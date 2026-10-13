package org.xmms.renascene;

import android.content.Context;
import android.graphics.Bitmap;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import org.json.JSONObject;
import java.io.File;
import java.io.FileInputStream;
import java.io.IOException;

/** Destructive cache tests reachable only through the debuggable probe Activity. */
final class XmmsArtworkCacheTests {
    private XmmsArtworkCacheTests() {}
    private static void check(boolean condition, String detail) throws IOException {
        if (!condition) throw new IOException(detail);
    }
    private static long[] totals(Context context) {
        File[] files = XmmsArtworkCache.directory(context).listFiles();
        long count = 0, bytes = 0;
        if (files != null) for (File file : files) { if (file.isFile()) { count++; bytes += file.length(); } }
        return new long[]{count, bytes};
    }
    static JSONObject run(Context context) throws Exception {
        check((context.getApplicationInfo().flags & android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE) != 0, "debug only");
        XmmsArtworkCache.pin(null);
        File directory = XmmsArtworkCache.directory(context);
        File[] old = directory.listFiles();
        if (old != null) for (File file : old) check(file.delete(), "cache cleanup");
        Bitmap image = Bitmap.createBitmap(512, 512, Bitmap.Config.ARGB_8888);
        image.eraseColor(0xffad2040);
        Uri first = XmmsArtworkCache.store(context, "test-first", image);
        // Open before eviction; Unix unlink must not break an existing reader.
        ParcelFileDescriptor descriptor = context.getContentResolver().openFileDescriptor(first, "r");
        check(descriptor != null, "initial descriptor");
        XmmsArtworkCache.pin(first);
        Uri second = XmmsArtworkCache.store(context, "test-second", image);
        XmmsArtworkCache.pin(second);
        for (int i = 0; i < 140; i++) XmmsArtworkCache.store(context, "test-normal-" + i, image);
        long[] normal = totals(context);
        check(normal[0] <= 128 && normal[1] <= 32L * 1024 * 1024, "normal bounds");
        check(new File(directory, first.getLastPathSegment() + ".png").isFile(), "five-minute grace lost");
        check(new File(directory, second.getLastPathSegment() + ".png").isFile(), "pin lost");
        // All recent published URIs have grace. Hard pressure must evict grace
        // early, while keeping the currently pinned object and open descriptor.
        for (int i = 0; i < 180; i++) {
            Uri uri = XmmsArtworkCache.store(context, "test-grace-" + i, image);
            XmmsArtworkCache.pin(uri);
            long[] value = totals(context);
            check(value[0] <= 160 && value[1] <= 40L * 1024 * 1024, "hard bounds");
        }
        check(!new File(directory, first.getLastPathSegment() + ".png").exists(), "grace pressure eviction");
        try (FileInputStream input = new FileInputStream(descriptor.getFileDescriptor())) {
            check(input.read() == 137 && input.read() == 80, "delayed descriptor unreadable");
        } finally { descriptor.close(); }
        // Exercise byte pressure, including abandoned temporary-file inventory.
        for (int i = 0; i < 25; i++) {
            try (java.io.RandomAccessFile file = new java.io.RandomAccessFile(new File(directory, String.format(java.util.Locale.ROOT, "%032x.tmp", i)), "rw")) {
                file.setLength(2L * 1024 * 1024);
            }
        }
        XmmsArtworkCache.store(context, "test-byte-pressure", image);
        long[] hard = totals(context);
        check(hard[0] <= 160 && hard[1] <= 40L * 1024 * 1024, "temporary byte bounds");
        Uri a = XmmsArtworkCache.store(context, "random-a", image);
        check(a.equals(XmmsArtworkCache.lookup(context, "random-a")), "full key lookup");
        Uri b = XmmsArtworkCache.store(context, "random-b", image);
        check(!a.equals(b) && XmmsArtworkCache.id(a) != null, "opaque mapping collision");
        JSONObject result = new JSONObject().put("normalCount", normal[0]).put("normalBytes", normal[1])
                .put("hardCount", hard[0]).put("hardBytes", hard[1]).put("delayedReader", true).put("gracePressure", true);
        XmmsArtworkCache.pin(null);
        return result;
    }
}
