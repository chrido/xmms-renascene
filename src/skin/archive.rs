//! Skin archive formats and bounded, selective archive readers.

use std::collections::HashSet;
use std::fs::File;
use std::io::{self, Read, Seek};
use std::path::Path;

use super::source::archive_asset_key;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ArchiveFormat {
    Zip,
    Tar,
    TarGz,
    TarBz2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ArchiveMatch<'a> {
    pub format: ArchiveFormat,
    /// Slice of the original name, retaining the caller's spelling and case.
    pub suffix: &'a str,
}

// Longest compound suffixes first. All recognition, naming, and decoding use
// this table; aliases map to the same decoder format.
const SUFFIXES: &[(&str, ArchiveFormat)] = &[
    (".tar.bz2", ArchiveFormat::TarBz2),
    (".tar.gz", ArchiveFormat::TarGz),
    (".tbz2", ArchiveFormat::TarBz2),
    (".tgz", ArchiveFormat::TarGz),
    (".zip", ArchiveFormat::Zip),
    (".wsz", ArchiveFormat::Zip),
    (".tar", ArchiveFormat::Tar),
];

pub(super) fn classify_name(name: &str) -> Option<ArchiveMatch<'_>> {
    // ASCII case folding preserves byte indices, including for non-ASCII names.
    let lower = name.to_ascii_lowercase();
    SUFFIXES.iter().find_map(|&(suffix, format)| {
        lower.strip_suffix(suffix).map(|stem| ArchiveMatch {
            format,
            suffix: &name[stem.len()..],
        })
    })
}

pub(super) fn classify_path(path: &Path) -> Option<ArchiveMatch<'_>> {
    classify_name(path.file_name()?.to_str()?)
}

/// Limits apply to all archive members, not just retained assets. A classic
/// skin's bitmaps are much smaller than 16 MiB; 128 MiB allows many overrides
/// while bounding decompression and in-memory payloads. Entry counts include
/// ZIP directories and TAR entries exposed by the tar crate. TAR's expanded-byte
/// limit covers the *entire* decompressed stream (headers, padding, and hidden
/// extension metadata); ZIP counts bytes actually expanded from every member,
/// including irrelevant files and duplicates.
///
/// ZIP's central directory is parsed by the zip crate before its entry count can
/// be checked; these limits cannot bound that parser's metadata allocation.
/// TAR extension metadata may likewise allocate within the stream-wide limit.
#[derive(Clone, Copy)]
struct Limits {
    max_asset_bytes: u64,
    max_total_bytes: u64,
    max_entries: usize,
}

const DEFAULT_LIMITS: Limits = Limits {
    max_asset_bytes: 16 * 1024 * 1024,
    max_total_bytes: 128 * 1024 * 1024,
    max_entries: 4096,
};

pub(super) fn entries(path: &Path) -> io::Result<Vec<(String, Vec<u8>)>> {
    entries_with_limits(path, DEFAULT_LIMITS)
}

fn entries_with_limits(path: &Path, limits: Limits) -> io::Result<Vec<(String, Vec<u8>)>> {
    let label = path.display().to_string();
    match classify_path(path).map(|matched| matched.format) {
        Some(ArchiveFormat::Zip) => {
            let file = File::open(path).map_err(|err| context(err, "open archive", &label))?;
            zip_archive_entries(file, &label, limits)
        }
        Some(ArchiveFormat::Tar) => {
            let file = File::open(path).map_err(|err| context(err, "open archive", &label))?;
            tar_archive_entries(file, &label, limits)
        }
        Some(ArchiveFormat::TarGz) => {
            let file = File::open(path).map_err(|err| context(err, "open archive", &label))?;
            tar_archive_entries(flate2::read::GzDecoder::new(file), &label, limits)
        }
        Some(ArchiveFormat::TarBz2) => {
            let file = File::open(path).map_err(|err| context(err, "open archive", &label))?;
            tar_archive_entries(bzip2::read::BzDecoder::new(file), &label, limits)
        }
        None => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported skin archive format: {label}"),
        )),
    }
}

fn invalid(label: &str, detail: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("{label}: {detail}"))
}

fn context(err: io::Error, operation: &str, label: &str) -> io::Error {
    io::Error::new(err.kind(), format!("{operation} {label}: {err}"))
}

fn check_count(count: usize, label: &str, limits: Limits) -> io::Result<()> {
    if count > limits.max_entries {
        return Err(invalid(label, "archive entry count exceeds limit"));
    }
    Ok(())
}

fn check_size(
    size: u64,
    retained: bool,
    total: u64,
    label: &str,
    limits: Limits,
) -> io::Result<()> {
    if retained && size > limits.max_asset_bytes {
        return Err(invalid(label, "skin asset exceeds per-asset byte limit"));
    }
    if total
        .checked_add(size)
        .is_none_or(|n| n > limits.max_total_bytes)
    {
        return Err(invalid(label, "expanded payload exceeds total byte limit"));
    }
    Ok(())
}

// Match Read::read_to_end's behavior: interruption is transient and consumes
// no bytes, so retry without changing either budget.
fn read_retry<R: Read>(reader: &mut R, buf: &mut [u8]) -> io::Result<usize> {
    loop {
        match reader.read(buf) {
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            result => return result,
        }
    }
}

// Only retained files allocate payload buffers. All files are read in bounded
// chunks, so inaccurate advertised sizes cannot bypass the actual byte limits.
fn read_payload<R: Read>(
    reader: &mut R,
    retained: bool,
    total: &mut u64,
    label: &str,
    limits: Limits,
) -> io::Result<Vec<u8>> {
    let mut contents = Vec::new();
    let mut file_bytes = 0u64;
    let mut buffer = [0u8; 8192];
    loop {
        let n = read_retry(reader, &mut buffer)
            .map_err(|err| context(err, "read archive entry", label))?;
        if n == 0 {
            return Ok(contents);
        }
        file_bytes = file_bytes
            .checked_add(n as u64)
            .ok_or_else(|| invalid(label, "skin asset byte count overflow"))?;
        *total = total
            .checked_add(n as u64)
            .ok_or_else(|| invalid(label, "expanded payload byte count overflow"))?;
        if retained && file_bytes > limits.max_asset_bytes {
            return Err(invalid(label, "skin asset exceeds per-asset byte limit"));
        }
        if *total > limits.max_total_bytes {
            return Err(invalid(label, "expanded payload exceeds total byte limit"));
        }
        if retained {
            contents.extend_from_slice(&buffer[..n]);
        }
    }
}

fn read_file<R: Read>(
    reader: &mut R,
    advertised: u64,
    selected: bool,
    total: &mut u64,
    label: &str,
    limits: Limits,
) -> io::Result<Vec<u8>> {
    check_size(advertised, selected, *total, label, limits)?;
    let before = *total;
    let contents = read_payload(reader, selected, total, label, limits)?;
    if *total - before != advertised {
        return Err(invalid(
            label,
            "archive entry size does not match advertised size",
        ));
    }
    Ok(contents)
}

fn zip_archive_entries<R: Read + Seek>(
    reader: R,
    label: &str,
    limits: Limits,
) -> io::Result<Vec<(String, Vec<u8>)>> {
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|err| context(zip_error(err), "open zip", label))?;
    check_count(archive.len(), label, limits)?;
    let mut retained = Vec::new();
    let mut seen = HashSet::new();
    let mut total = 0u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| context(zip_error(err), "read zip entry", label))?;
        let name = entry.name().to_owned();
        let entry_label = format!("{label}:{name}");
        // Even a directory entry can carry data in a malformed ZIP; count it.
        // Drain duplicates and irrelevant members rather than trusting their
        // advertised sizes: they count against the actual expansion budget too.
        let selected =
            !entry.is_dir() && archive_asset_key(&name).is_some_and(|key| seen.insert(key));
        let size = entry.size();
        let contents = read_file(&mut entry, size, selected, &mut total, &entry_label, limits)?;
        if selected {
            retained.push((name, contents));
        }
    }
    Ok(retained)
}

// A cap around the decompressed TAR stream also covers bytes the tar crate
// skips internally (padding and PAX/GNU extension records). Probe one byte at
// the boundary to distinguish an exact-size stream from an oversized stream.
struct BoundedReader<R> {
    inner: R,
    remaining: u64,
}

impl<R: Read> Read for BoundedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.remaining == 0 {
            if read_retry(&mut self.inner, &mut [0u8; 1])? == 0 {
                return Ok(0);
            }
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "decompressed TAR stream exceeds total byte limit",
            ));
        }
        let allowed = buf
            .len()
            .min(usize::try_from(self.remaining).unwrap_or(usize::MAX));
        let n = read_retry(&mut self.inner, &mut buf[..allowed])?;
        self.remaining -= n as u64;
        Ok(n)
    }
}

fn tar_archive_entries<R: Read>(
    reader: R,
    label: &str,
    limits: Limits,
) -> io::Result<Vec<(String, Vec<u8>)>> {
    let bounded = BoundedReader {
        inner: reader,
        remaining: limits.max_total_bytes,
    };
    let mut archive = tar::Archive::new(bounded);
    let mut retained = Vec::new();
    let mut seen = HashSet::new();
    let mut total = 0u64;
    let entries = archive
        .entries()
        .map_err(|err| context(err, "read tar", label))?;
    for (index, entry) in entries.enumerate() {
        let mut entry = entry.map_err(|err| context(err, "read tar entry", label))?;
        check_count(index + 1, label, limits)?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let name = entry
            .path()
            .map_err(|err| context(err, "read tar entry path", label))?
            .to_string_lossy()
            .into_owned();
        let entry_label = format!("{label}:{name}");
        let selected = archive_asset_key(&name).is_some_and(|key| seen.insert(key));
        let size = entry.size();
        let contents = read_file(&mut entry, size, selected, &mut total, &entry_label, limits)?;
        if selected {
            retained.push((name, contents));
        }
    }
    // Verify the remainder as well, including trailing blocks and compressed
    // checksums; tar iteration may stop before the decompressor reaches EOF.
    io::copy(&mut archive.into_inner(), &mut io::sink())
        .map_err(|err| context(err, "finish tar", label))?;
    Ok(retained)
}

fn zip_error(err: zip::result::ZipError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_ARCHIVE: AtomicUsize = AtomicUsize::new(0);

    fn limits(asset: u64, total: u64, count: usize) -> Limits {
        Limits {
            max_asset_bytes: asset,
            max_total_bytes: total,
            max_entries: count,
        }
    }

    fn zip_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for &(name, contents) in files {
            zip.start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
            zip.write_all(contents).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    fn tar_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut tar = tar::Builder::new(Vec::new());
        for &(name, contents) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, name, Cursor::new(contents))
                .unwrap();
        }
        tar.into_inner().unwrap()
    }

    fn assert_limited<T: std::fmt::Debug>(result: io::Result<T>, context: &str) {
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData, "{err}");
        assert!(err.to_string().contains(context), "{err}");
    }

    struct InterruptAt<'a> {
        cursor: Cursor<&'a [u8]>,
        position: u64,
        interruptions: usize,
    }

    impl<'a> InterruptAt<'a> {
        fn new(bytes: &'a [u8], position: u64) -> Self {
            Self {
                cursor: Cursor::new(bytes),
                position,
                interruptions: 0,
            }
        }
    }

    impl Read for InterruptAt<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.interruptions == 0 && self.cursor.position() == self.position {
                self.interruptions += 1;
                return Err(io::ErrorKind::Interrupted.into());
            }
            self.cursor.read(buf)
        }
    }

    #[test]
    fn zip_selected_entry_and_count_boundaries() {
        let bytes = zip_bytes(&[("Skin/Main.XPM", b"abcd")]);
        let load = |limits| zip_archive_entries(Cursor::new(&bytes), "test.zip", limits);
        assert_eq!(
            load(limits(4, 4, 1)).unwrap(),
            vec![("Skin/Main.XPM".into(), b"abcd".to_vec())]
        );
        assert_limited(load(limits(3, 4, 1)), "test.zip:Skin/Main.XPM");
        assert_limited(load(limits(4, 3, 1)), "total byte limit");
        assert_limited(load(limits(4, 4, 0)), "entry count");
    }

    #[test]
    fn zip_irrelevant_and_duplicate_payloads_are_bounded_but_not_retained() {
        let bytes = zip_bytes(&[
            ("A/MAIN.XPM", b"first"),
            ("B/main.xpm", b"last"),
            ("junk.bin", b"xyz"),
        ]);
        let load = |limits| zip_archive_entries(Cursor::new(&bytes), "test.zip", limits);
        assert_eq!(
            load(limits(5, 12, 3)).unwrap(),
            vec![("A/MAIN.XPM".into(), b"first".to_vec())]
        );
        // Per-asset cap applies only to selected files, but both discarded
        // payloads count toward the actual expanded-byte budget.
        assert_limited(load(limits(5, 11, 3)), "total byte limit");
        assert_limited(load(limits(5, 12, 2)), "entry count");
        let irrelevant = zip_bytes(&[("junk.bin", b"12345")]);
        assert!(
            zip_archive_entries(Cursor::new(&irrelevant), "test.zip", limits(1, 5, 1))
                .unwrap()
                .is_empty()
        );
        assert_limited(
            zip_archive_entries(Cursor::new(&irrelevant), "test.zip", limits(1, 4, 1)),
            "total byte limit",
        );
        // Count directory records, and even drain their payload if present.
        let directory_with_data = zip_bytes(&[("OddDirectory/", b"xy")]);
        assert!(zip_archive_entries(
            Cursor::new(&directory_with_data),
            "test.zip",
            limits(1, 2, 1)
        )
        .unwrap()
        .is_empty());
        assert_limited(
            zip_archive_entries(
                Cursor::new(&directory_with_data),
                "test.zip",
                limits(1, 1, 1),
            ),
            "total byte limit",
        );
        assert_limited(
            zip_archive_entries(
                Cursor::new(&directory_with_data),
                "test.zip",
                limits(1, 2, 0),
            ),
            "entry count",
        );
    }

    #[test]
    fn only_known_images_and_metadata_are_retained() {
        let files: &[(&str, &[u8])] = &[
            ("skin/Main.jpeg", b"x"),
            ("skin/other.xpm", b"x"),
            ("skin/Numbers.PNG", b"1"),
            ("skin/nums_ex.XPM", b"2"),
            ("skin/EqMain.bmp", b"3"),
            ("skin/VISCOLOR.TXT", b"4"),
            ("skin/PlEdit.TxT", b"5"),
            ("skin/REGION.txt", b"6"),
            ("skin/other.txt", b"x"),
        ];
        let zip = zip_bytes(files);
        let retained = zip_archive_entries(Cursor::new(zip), "test.zip", limits(1, 9, 9)).unwrap();
        assert_eq!(
            retained
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            [
                "skin/Numbers.PNG",
                "skin/nums_ex.XPM",
                "skin/EqMain.bmp",
                "skin/VISCOLOR.TXT",
                "skin/PlEdit.TxT",
                "skin/REGION.txt"
            ]
        );
    }

    #[test]
    fn actual_reads_are_bounded_even_when_a_size_hint_is_wrong() {
        let mut total = 0;
        assert_limited(
            read_payload(
                &mut Cursor::new(b"12345"),
                true,
                &mut total,
                "test.zip:main.xpm",
                limits(4, 10, 1),
            ),
            "per-asset",
        );
        let mut total = 0;
        assert_limited(
            read_payload(
                &mut Cursor::new(b"12345"),
                false,
                &mut total,
                "test.zip:junk",
                limits(1, 4, 1),
            ),
            "total byte limit",
        );
    }

    #[test]
    fn read_file_checks_actual_sizes_and_budgets_for_selected_and_discarded_data() {
        let data = b"abcd";
        for selected in [true, false] {
            let label = if selected {
                "skin:Main.xpm"
            } else {
                "skin:ignored.bin"
            };
            let mut total = 0;
            let bytes = read_file(
                &mut Cursor::new(data),
                4,
                selected,
                &mut total,
                label,
                limits(4, 4, 1),
            )
            .unwrap();
            assert_eq!(bytes, if selected { data.to_vec() } else { Vec::new() });
            assert_eq!(total, 4);

            // Advertised smaller or larger than the stream: the actual read
            // must be checked for both selected and discarded entries.
            let mut total = 0;
            let err = read_file(
                &mut Cursor::new(data),
                2,
                selected,
                &mut total,
                label,
                limits(4, 4, 1),
            )
            .unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidData);
            assert!(err.to_string().contains(label), "{err}");
            assert!(err.to_string().contains("advertised size"), "{err}");
            let mut total = 0;
            assert_limited(
                read_file(
                    &mut Cursor::new(data),
                    5,
                    selected,
                    &mut total,
                    label,
                    limits(5, 5, 1),
                ),
                "advertised size",
            );

            let mut total = 0;
            assert_limited(
                read_file(
                    &mut Cursor::new(data),
                    2,
                    selected,
                    &mut total,
                    label,
                    limits(4, 3, 1),
                ),
                "total byte limit",
            );
            // The advertised byte count fits the remaining total, but the
            // actual count does not (including a nonzero prior total).
            let mut total = 2;
            assert_limited(
                read_file(
                    &mut Cursor::new(b"ab"),
                    1,
                    selected,
                    &mut total,
                    label,
                    limits(4, 3, 1),
                ),
                "total byte limit",
            );
        }
        let mut total = 0;
        assert_limited(
            read_file(
                &mut Cursor::new(data),
                2,
                true,
                &mut total,
                "skin:Main.xpm",
                limits(3, 4, 1),
            ),
            "per-asset",
        );
    }

    #[test]
    fn interrupted_payload_reads_and_tar_boundary_probes_are_retried() {
        for selected in [true, false] {
            let mut reader = InterruptAt::new(b"abcd", 0);
            let mut total = 0;
            let result = read_file(
                &mut reader,
                4,
                selected,
                &mut total,
                "skin:Main.xpm",
                limits(4, 4, 1),
            )
            .unwrap();
            assert_eq!(
                result,
                if selected {
                    b"abcd".to_vec()
                } else {
                    Vec::new()
                }
            );
            assert_eq!(total, 4);
            assert_eq!(reader.interruptions, 1);
        }

        let mut exact = BoundedReader {
            inner: InterruptAt::new(b"ab", 2),
            remaining: 2,
        };
        let mut buf = [0; 4];
        assert_eq!(exact.read(&mut buf).unwrap(), 2);
        assert_eq!(exact.read(&mut buf).unwrap(), 0); // interrupted EOF probe
        assert_eq!(exact.inner.interruptions, 1);
        assert_eq!(exact.remaining, 0);

        let mut oversized = BoundedReader {
            inner: InterruptAt::new(b"abc", 2),
            remaining: 2,
        };
        assert_eq!(oversized.read(&mut buf).unwrap(), 2);
        assert_limited(oversized.read(&mut buf), "total byte limit");
        assert_eq!(oversized.inner.interruptions, 1);
        assert_eq!(oversized.remaining, 0);

        let mut interrupted_data = BoundedReader {
            inner: InterruptAt::new(b"ab", 0),
            remaining: 2,
        };
        assert_eq!(interrupted_data.read(&mut buf).unwrap(), 2);
        assert_eq!(interrupted_data.inner.interruptions, 1);
        assert_eq!(interrupted_data.remaining, 0);
    }

    #[test]
    fn tar_stream_and_entry_limits_include_irrelevant_data_and_padding() {
        let bytes = tar_bytes(&[("Main.xpm", b"abc"), ("extra.bin", b"123456")]);
        let length = bytes.len() as u64;
        assert_eq!(&bytes[512 + 512 + 512..512 + 512 + 512 + 3], b"123");
        let load = |limits| tar_archive_entries(Cursor::new(&bytes), "test.tar", limits);
        assert_eq!(
            load(limits(3, length, 2)).unwrap(),
            vec![("Main.xpm".into(), b"abc".to_vec())]
        );
        assert_limited(load(limits(2, length, 2)), "per-asset");
        assert_limited(load(limits(3, length - 1, 2)), "total byte limit");
        // Header + padded first file + next header + three bytes of the
        // ignored file: exhaust the stream budget *inside* skipped data.
        assert_limited(load(limits(3, 512 + 512 + 512 + 3, 2)), "total byte limit");
        assert_limited(load(limits(3, length, 1)), "entry count");

        let irrelevant = tar_bytes(&[("extra.bin", b"123456")]);
        let length = irrelevant.len() as u64;
        assert!(
            tar_archive_entries(Cursor::new(&irrelevant), "test.tar", limits(1, length, 1))
                .unwrap()
                .is_empty()
        );
        assert_limited(
            tar_archive_entries(
                Cursor::new(&irrelevant),
                "test.tar",
                limits(1, length - 1, 1),
            ),
            "total byte limit",
        );
    }

    #[test]
    fn tar_extension_metadata_is_covered_by_stream_limit() {
        let long_name = format!("{}/Main.xpm", "nested".repeat(30));
        let bytes = tar_bytes(&[(&long_name, b"ok")]);
        assert!(
            matches!(bytes[156], b'L' | b'x'),
            "expected a long-path extension record"
        );
        let length = bytes.len() as u64;
        let loaded =
            tar_archive_entries(Cursor::new(&bytes), "test.tar", limits(2, length, 1)).unwrap();
        assert_eq!(loaded, vec![(long_name.clone(), b"ok".to_vec())]);
        assert_limited(
            tar_archive_entries(Cursor::new(&bytes), "test.tar", limits(2, length - 1, 1)),
            "total byte limit",
        );
        // The first header is followed by GNU/PAX long-path metadata, so
        // 513 bytes stops inside that metadata rather than final padding.
        assert!(long_name.len() > 100);
        assert_limited(
            tar_archive_entries(Cursor::new(&bytes), "test.tar", limits(2, 512 + 1, 1)),
            "total byte limit",
        );
    }

    #[test]
    fn compressed_tar_decoders_use_the_same_stream_limit() {
        let raw = tar_bytes(&[("Main.xpm", b"ok"), ("ignored.bin", b"stuff")]);
        let root = std::env::temp_dir().join(format!(
            "xmms-skin-archive-bounds-{}-{}",
            std::process::id(),
            NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let gz_path = root.join("skin.TAR.GZ");
        let bz_path = root.join("skin.tbz2");
        let mut gz = flate2::write::GzEncoder::new(
            File::create(&gz_path).unwrap(),
            flate2::Compression::default(),
        );
        gz.write_all(&raw).unwrap();
        gz.finish().unwrap();
        let mut bz = bzip2::write::BzEncoder::new(
            File::create(&bz_path).unwrap(),
            bzip2::Compression::default(),
        );
        bz.write_all(&raw).unwrap();
        bz.finish().unwrap();
        for path in [&gz_path, &bz_path] {
            assert_eq!(
                entries_with_limits(path, limits(2, raw.len() as u64, 2))
                    .unwrap()
                    .len(),
                1
            );
            assert_limited(
                entries_with_limits(path, limits(2, raw.len() as u64 - 1, 2)),
                "total byte limit",
            );
            let mut truncated = std::fs::read(path).unwrap();
            truncated.truncate(truncated.len() - 8);
            std::fs::write(path, truncated).unwrap();
            let err = entries_with_limits(path, limits(2, raw.len() as u64, 2)).unwrap_err();
            assert!(
                err.to_string().contains(&path.display().to_string()),
                "{err}"
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn corrupt_truncated_archives_and_stream_read_errors_are_reported() {
        let mut zip = zip_bytes(&[("ignored.bin", b"unique-payload")]);
        let start = zip
            .windows(b"unique-payload".len())
            .position(|window| window == b"unique-payload")
            .unwrap();
        zip[start] ^= 1; // Incorrect CRC in an irrelevant ZIP entry must not be ignored.
        let err =
            zip_archive_entries(Cursor::new(&zip), "broken.zip", limits(20, 20, 1)).unwrap_err();
        assert!(err.to_string().contains("broken.zip:ignored.bin"), "{err}");
        let zip = zip_bytes(&[("Main.xpm", b"abc")]);
        assert!(zip_archive_entries(
            Cursor::new(&zip[..zip.len() - 10]),
            "short.zip",
            limits(3, 3, 1)
        )
        .is_err());

        let mut tar = tar_bytes(&[("Main.xpm", b"abcd")]);
        tar.truncate(512 + 3); // Cut through the selected file data.
        let err =
            tar_archive_entries(Cursor::new(&tar), "short.tar", limits(4, 4096, 1)).unwrap_err();
        assert!(err.to_string().contains("short.tar:Main.xpm"), "{err}");

        struct FailAfter<R> {
            inner: R,
            remaining: usize,
        }
        impl<R: Read> Read for FailAfter<R> {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                if self.remaining == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "injected read failure",
                    ));
                }
                let allowed = buf.len().min(self.remaining);
                let n = self.inner.read(&mut buf[..allowed])?;
                self.remaining -= n;
                Ok(n)
            }
        }
        let bytes = tar_bytes(&[("Main.xpm", b"abcd")]);
        let err = tar_archive_entries(
            FailAfter {
                inner: Cursor::new(bytes),
                remaining: 513,
            },
            "broken.tar",
            limits(4, 20000, 1),
        )
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
        assert!(err.to_string().contains("broken.tar"), "{err}");
    }
}
