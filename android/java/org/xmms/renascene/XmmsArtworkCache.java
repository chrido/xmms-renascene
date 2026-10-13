package org.xmms.renascene;

import android.content.Context;
import android.graphics.Bitmap;
import android.net.Uri;
import android.os.SystemClock;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.security.SecureRandom;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

/** Worker-only immutable PNG storage. No song/path-derived public URI IDs. */
final class XmmsArtworkCache {
    private static final long NORMAL_BYTES = 32L * 1024 * 1024;
    private static final long HARD_BYTES = 40L * 1024 * 1024;
    private static final int NORMAL_ENTRIES = 128;
    private static final int HARD_ENTRIES = 160;
    private static final long GRACE_MS = 5 * 60 * 1000;
    private static final SecureRandom RANDOM = new SecureRandom();
    private static final Map<String, String> KEYS = new ConcurrentHashMap<>();
    private static final Map<String, Long> GRACE = new ConcurrentHashMap<>();
    private static final Object IO_LOCK = new Object();
    private static volatile String pinned;
    private static long lastRecoveryMs;
    private static final java.util.concurrent.atomic.AtomicLong PNG_WRITES = new java.util.concurrent.atomic.AtomicLong();
    private static final java.util.concurrent.atomic.AtomicLong PNG_NS = new java.util.concurrent.atomic.AtomicLong();
    private static final java.util.concurrent.atomic.AtomicLong NATIVE_PNG_NS = new java.util.concurrent.atomic.AtomicLong();
    private static final java.util.concurrent.atomic.AtomicLong PNG_BUFFER_PEAK = new java.util.concurrent.atomic.AtomicLong();
    private static final java.util.concurrent.atomic.AtomicLong OPEN_CALLS = new java.util.concurrent.atomic.AtomicLong();
    private static final java.util.concurrent.atomic.AtomicLong OPEN_NS = new java.util.concurrent.atomic.AtomicLong();
    private static volatile long cacheBytes;
    private static volatile int cacheEntries;
    private static final String RECOVER = "org.xmms.renascene.service.ARTWORK_RECOVER";

    private XmmsArtworkCache() {}

    static File directory(Context application) {
        return new File(application.getCacheDir(), "media-artwork");
    }

    static Uri uri(Context application, String id) {
        return new Uri.Builder().scheme("content").authority(application.getPackageName() + ".artwork")
                .appendPath("artwork").appendPath(id).build();
    }

    static String id(Uri uri) {
        if (!"content".equals(uri.getScheme()) || uri.getPathSegments().size() != 2
                || !"artwork".equals(uri.getPathSegments().get(0))) return null;
        String id = uri.getPathSegments().get(1);
        return id.matches("[0-9a-f]{32}") ? id : null;
    }

    // No disk I/O: publication pins by opaque identifier on the main thread.
    static synchronized void pin(Uri uri) {
        String next = uri == null ? null : id(uri);
        if (pinned != null && !pinned.equals(next)) GRACE.put(pinned, SystemClock.elapsedRealtime() + GRACE_MS);
        pinned = next;
        long now = SystemClock.elapsedRealtime();
        GRACE.entrySet().removeIf(entry -> entry.getValue() <= now);
        while (GRACE.size() > HARD_ENTRIES) {
            String oldest = GRACE.entrySet().stream().min(Map.Entry.comparingByValue()).get().getKey();
            GRACE.remove(oldest);
        }
    }

    static void recover(Context application, String id) {
        synchronized (XmmsArtworkCache.class) {
            long now = SystemClock.elapsedRealtime();
            if (id == null || !id.equals(pinned) || now - lastRecoveryMs < 3000) return;
            lastRecoveryMs = now;
            KEYS.values().removeIf(value -> value.equals(id));
        }
        // Binder IPC must not hold the short main-thread lookup/pin monitor.
        application.sendBroadcast(new android.content.Intent(RECOVER).setPackage(application.getPackageName()));
    }

    static synchronized Uri lookup(Context application, String key) {
        String id = KEYS.get(key);
        // File existence is deliberately not checked on the calling main thread.
        return id == null ? null : uri(application, id);
    }

    static void noteNativePng(long startedNs, byte[] png) {
        NATIVE_PNG_NS.addAndGet(SystemClock.elapsedRealtimeNanos() - startedNs);
        if (png != null) PNG_BUFFER_PEAK.accumulateAndGet(png.length, Math::max);
    }

    static void noteOpen(long startedNs) {
        OPEN_CALLS.incrementAndGet(); OPEN_NS.addAndGet(SystemClock.elapsedRealtimeNanos() - startedNs);
    }

    static org.json.JSONObject metrics() {
        org.json.JSONObject value = new org.json.JSONObject();
        try {
            value.put("pngWrites", PNG_WRITES.get()).put("pngEncodeWriteNs", PNG_NS.get())
                .put("nativePngRenderEncodeNs", NATIVE_PNG_NS.get()).put("pngByteArrayPeak", PNG_BUFFER_PEAK.get())
                .put("providerOpens", OPEN_CALLS.get()).put("providerOpenNs", OPEN_NS.get())
                .put("diskCacheBytes", cacheBytes).put("diskCacheEntries", cacheEntries)
                .put("uriMappings", KEYS.size()).put("graceMappings", GRACE.size());
        } catch (org.json.JSONException impossible) { throw new IllegalStateException(impossible); }
        return value;
    }

    static Uri store(Context application, String key, Bitmap image) throws IOException {
        if (android.os.Looper.myLooper() == android.os.Looper.getMainLooper()) {
            throw new IllegalStateException("artwork cache I/O on main");
        }
        // Debug cache-pressure test helper; production supplies native PNG bytes.
        java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream();
        if (!image.compress(Bitmap.CompressFormat.PNG, 100, bytes)) throw new IOException("artwork encode failed");
        return storePng(application, key, bytes.toByteArray());
    }

    static Uri storePng(Context application, String key, byte[] png) throws IOException {
        if (android.os.Looper.myLooper() == android.os.Looper.getMainLooper()) throw new IllegalStateException("artwork cache I/O on main");
        if (png.length == 0 || png.length > 1024 * 1024) throw new IOException("artwork PNG exceeds limit");
        synchronized (IO_LOCK) { return storeIO(application, key, png); }
    }

    private static Uri storeIO(Context application, String key, byte[] png) throws IOException {
        if (android.os.Looper.myLooper() == android.os.Looper.getMainLooper()) {
            throw new IllegalStateException("artwork cache I/O on main");
        }
        File directory = directory(application);
        if (!directory.isDirectory() && !directory.mkdirs()) throw new IOException("artwork cache directory unavailable");
        String existing = KEYS.get(key);
        if (existing != null && new File(directory, existing + ".png").isFile()) {
            android.graphics.BitmapFactory.Options bounds = new android.graphics.BitmapFactory.Options();
            bounds.inJustDecodeBounds = true;
            android.graphics.BitmapFactory.decodeFile(new File(directory, existing + ".png").getPath(), bounds);
            if (bounds.outWidth == 512 && bounds.outHeight == 512) return uri(application, existing);
            new File(directory, existing + ".png").delete(); KEYS.remove(key);
        }
        byte[] bytes = new byte[16]; RANDOM.nextBytes(bytes);
        StringBuilder name = new StringBuilder(32);
        for (byte value : bytes) name.append(String.format(java.util.Locale.ROOT, "%02x", value & 255));
        String id = name.toString();
        // Reserve one maximum-sized temporary object before writing it. Hard
        // totals include in-flight PNG data, not just the final post-trim files.
        trim(directory, id, 2L * 1024 * 1024, 1);
        File temp = new File(directory, id + ".tmp");
        File output = new File(directory, id + ".png");
        long encodeStartedNs = SystemClock.elapsedRealtimeNanos();
        try (FileOutputStream stream = new FileOutputStream(temp)) {
            stream.write(png);
            stream.getFD().sync();
        } catch (IOException failure) { temp.delete(); throw failure; }
        PNG_WRITES.incrementAndGet(); PNG_NS.addAndGet(SystemClock.elapsedRealtimeNanos() - encodeStartedNs);
        if (!temp.renameTo(output)) { temp.delete(); throw new IOException("artwork atomic rename failed"); }
        if (KEYS.size() >= HARD_ENTRIES && !KEYS.containsKey(key)) {
            String old = KEYS.entrySet().stream().filter(entry -> !entry.getValue().equals(pinned))
                    .map(Map.Entry::getKey).findFirst().orElse(null);
            if (old != null) KEYS.remove(old);
        }
        KEYS.put(key, id);
        try { trim(directory, id, 0, 0); }
        catch (IOException failure) { KEYS.remove(key); output.delete(); throw failure; }
        return uri(application, id);
    }

    private static void trim(File directory, String justCreated, long reservedBytes, int reservedEntries) throws IOException {
        File[] listed = directory.listFiles();
        if (listed == null) throw new IOException("artwork cache inventory failed");
        ArrayList<File> files = new ArrayList<>();
        long bytes = reservedBytes;
        for (File file : listed) {
            if (!file.isFile()) continue;
            files.add(file); bytes += file.length();
        }
        files.sort(Comparator.comparingLong(File::lastModified));
        int count = files.size() + reservedEntries;
        long now = SystemClock.elapsedRealtime();
        for (int pass = 0; pass < 2; pass++) {
            for (File file : files) {
                if (!file.exists()) continue;
                String id = file.getName().replaceFirst("\\.(png|tmp)$", "");
                boolean hard = bytes > HARD_BYTES || count > HARD_ENTRIES;
                if (!hard && bytes <= NORMAL_BYTES && count <= NORMAL_ENTRIES) {
                    cacheBytes = bytes - reservedBytes; cacheEntries = count - reservedEntries; return;
                }
                if (id.equals(pinned) || id.equals(justCreated)) continue;
                if (pass == 0 && GRACE.getOrDefault(id, 0L) > now) continue;
                if (pass == 1 && !hard) continue;
                long size = file.length();
                if (file.delete()) {
                    bytes -= size; count--; GRACE.remove(id);
                    KEYS.values().removeIf(value -> value.equals(id));
                }
            }
        }
        cacheBytes = bytes - reservedBytes; cacheEntries = count - reservedEntries;
        if (bytes > HARD_BYTES || count > HARD_ENTRIES) throw new IOException("artwork hard cache limit exceeded");
    }
}
