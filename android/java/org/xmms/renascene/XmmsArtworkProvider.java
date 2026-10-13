package org.xmms.renascene;

import android.content.ContentProvider;
import android.content.ContentResolver;
import android.content.ContentValues;
import android.content.res.AssetFileDescriptor;
import android.database.Cursor;
import android.net.Uri;
import android.os.Bundle;
import android.os.ParcelFileDescriptor;
import android.util.Size;

import java.io.File;
import java.io.FileNotFoundException;
import java.io.IOException;

/** Exported read-only artwork objects, not arbitrary app-private files.
 * Generated media images are intentionally readable by consumer apps that know
 * their random metadata URI. Paths/song titles are never used as public IDs. */
public final class XmmsArtworkProvider extends ContentProvider {
    @Override public boolean onCreate() { return true; }

    private File object(Uri uri) throws FileNotFoundException {
        String id = XmmsArtworkCache.id(uri);
        if (id == null || getContext() == null || !(getContext().getPackageName() + ".artwork").equals(uri.getAuthority())) {
            throw new FileNotFoundException("Unknown artwork object");
        }
        File directory = XmmsArtworkCache.directory(getContext());
        File file = new File(directory, id + ".png");
        try {
            // Refuse symlinks/canonical escape even if a debug tool manipulates
            // this otherwise app-private cache directory.
            if (!directory.getCanonicalFile().equals(new File(getContext().getCacheDir().getCanonicalFile(), "media-artwork"))
                    || !file.getCanonicalPath().equals(new File(directory.getCanonicalFile(), id + ".png").getPath())
                    || !file.isFile()) {
                if (!file.exists()) XmmsArtworkCache.recover(getContext().getApplicationContext(), id);
                throw new FileNotFoundException("Unknown artwork object");
            }
        } catch (IOException error) { throw new FileNotFoundException("Artwork unavailable"); }
        if (!validPng(file)) {
            XmmsArtworkCache.recover(getContext().getApplicationContext(), id);
            throw new FileNotFoundException("Corrupt artwork object");
        }
        return file;
    }

    // Bounded integrity checks, not image decoding/rasterization. Validate all
    // PNG chunk CRCs so a truncated/corrupt fixed cache object fails cleanly.
    private boolean validPng(File file) {
        if (file.length() > 2L * 1024 * 1024) return false;
        try (java.io.DataInputStream input = new java.io.DataInputStream(new java.io.BufferedInputStream(new java.io.FileInputStream(file)))) {
            if (input.readLong() != 0x89504e470d0a1a0aL) return false;
            byte[] buffer = new byte[8192];
            long remaining = file.length() - 8;
            for (int chunks = 0; chunks < 1024; chunks++) {
                int length = input.readInt(); int type = input.readInt(); remaining -= 12;
                if (length < 0 || length > remaining) return false;
                java.util.zip.CRC32 crc = new java.util.zip.CRC32();
                crc.update((type >>> 24) & 255); crc.update((type >>> 16) & 255);
                crc.update((type >>> 8) & 255); crc.update(type & 255);
                if (chunks == 0) {
                    if (type != 0x49484452 || length != 13) return false;
                    input.readFully(buffer, 0, 13); crc.update(buffer, 0, 13);
                    java.nio.ByteBuffer header = java.nio.ByteBuffer.wrap(buffer, 0, 13);
                    if (header.getInt() != 512 || header.getInt() != 512) return false;
                } else {
                    int left = length;
                    while (left > 0) {
                        int size = Math.min(left, buffer.length);
                        input.readFully(buffer, 0, size); crc.update(buffer, 0, size); left -= size;
                    }
                }
                remaining -= length;
                if (input.readInt() != (int) crc.getValue()) return false;
                if (type == 0x49454e44) return length == 0 && remaining == 0;
            }
        } catch (IOException corrupt) { return false; }
        return false;
    }

    @Override public ParcelFileDescriptor openFile(Uri uri, String mode) throws FileNotFoundException {
        if (!"r".equals(mode)) throw new FileNotFoundException("Artwork is read only");
        long startedNs = android.os.SystemClock.elapsedRealtimeNanos();
        try { return ParcelFileDescriptor.open(object(uri), ParcelFileDescriptor.MODE_READ_ONLY); }
        finally { XmmsArtworkCache.noteOpen(startedNs); }
    }

    @Override public AssetFileDescriptor openTypedAssetFile(Uri uri, String filter, Bundle options) throws FileNotFoundException {
        if (!("image/png".equals(filter) || "image/*".equals(filter) || "*/*".equals(filter))) {
            throw new FileNotFoundException("Unsupported artwork type");
        }
        Object requested = options == null ? null : options.get(ContentResolver.EXTRA_SIZE);
        if (requested != null) {
            if (!(requested instanceof Size)) throw new FileNotFoundException("Invalid artwork size");
            Size size = (Size) requested;
            if (size.getWidth() <= 0 || size.getHeight() <= 0 || size.getWidth() > 4096 || size.getHeight() > 4096) {
                throw new FileNotFoundException("Artwork size exceeds limit");
            }
        }
        // Fixed cached 512px master; consumers scale to requested display size.
        // Do not decode/resize or schedule work from a synchronous provider call.
        return new AssetFileDescriptor(openFile(uri, "r"), 0, AssetFileDescriptor.UNKNOWN_LENGTH);
    }

    @Override public String getType(Uri uri) { return XmmsArtworkCache.id(uri) == null ? null : "image/png"; }
    @Override public Cursor query(Uri uri, String[] projection, String selection, String[] args, String order) {
        throw new UnsupportedOperationException("Artwork objects are not enumerable");
    }
    @Override public Uri insert(Uri uri, ContentValues values) { throw new UnsupportedOperationException("Read only"); }
    @Override public int update(Uri uri, ContentValues values, String selection, String[] args) { throw new UnsupportedOperationException("Read only"); }
    @Override public int delete(Uri uri, String selection, String[] args) { throw new UnsupportedOperationException("Read only"); }
}
