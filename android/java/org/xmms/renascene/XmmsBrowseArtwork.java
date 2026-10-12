package org.xmms.renascene;

import android.content.Context;
import android.net.Uri;
import android.os.Handler;
import android.os.SystemClock;
import org.json.JSONObject;
import org.json.JSONException;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;

/** Lazy URI-only browse pictures: one running + latest pending, no playlist prefetch. */
final class XmmsBrowseArtwork {
    private static native String nativeSnapshot(String filesDir, int index);
    private static native String nativePrepare(String filesDir, int index);
    private static native byte[] nativeRender(String filesDir, int index, String frozen);
    interface Listener { void changed(); }
    private final Context application;
    private final String filesDir;
    private final Handler main;
    private Listener listener;
    private final Object lock = new Object();
    private final ThreadPoolExecutor worker = new ThreadPoolExecutor(1, 1, 0, TimeUnit.MILLISECONDS, new ArrayBlockingQueue<Runnable>(1));
    private int pending = -1;
    private boolean draining;
    private volatile boolean closed;
    private final java.util.concurrent.atomic.AtomicLong attempts = new java.util.concurrent.atomic.AtomicLong();
    private final java.util.concurrent.atomic.AtomicLong renders = new java.util.concurrent.atomic.AtomicLong();
    private final java.util.concurrent.atomic.AtomicLong cancelled = new java.util.concurrent.atomic.AtomicLong();
    private final java.util.concurrent.atomic.AtomicLong coalesced = new java.util.concurrent.atomic.AtomicLong();
    private final java.util.concurrent.atomic.AtomicInteger active = new java.util.concurrent.atomic.AtomicInteger();
    private final java.util.concurrent.atomic.AtomicInteger peakActive = new java.util.concurrent.atomic.AtomicInteger();
    private final java.util.concurrent.ConcurrentHashMap<String, Long> failedUntil = new java.util.concurrent.ConcurrentHashMap<>();

    XmmsBrowseArtwork(Context application, String filesDir, Handler main, Listener listener) {
        this.application = application.getApplicationContext(); this.filesDir = filesDir;
        this.main = main; this.listener = listener;
    }

    Uri icon(int index, boolean request) {
        if (closed) return null;
        String raw = nativeSnapshot(filesDir, index);
        if (raw == null) return null;
        try {
            JSONObject value = new JSONObject(raw);
            if (value.getBoolean("resolved") && !value.getBoolean("enabled")) return null;
            Uri cached = XmmsArtworkCache.lookup(application, value.getString("key") + ":512");
            if (cached != null) return cached;
            if (failedUntil.getOrDefault(value.getString("key"), 0L) > SystemClock.uptimeMillis()) return null;
        } catch (JSONException invalid) { return null; }
        if (!request) return null;
        synchronized (lock) {
            if (pending >= 0) coalesced.incrementAndGet();
            pending = index;
            if (!draining) { draining = true; worker.execute(this::drain); }
        }
        return null; // first result keeps app icon; notifyChildrenChanged later
    }

    private void drain() {
        while (!closed) {
            int index;
            synchronized (lock) {
                index = pending; pending = -1;
                if (index < 0) { draining = false; return; }
            }
            boolean acquired = false;
            String key = null;
            boolean published = false;
            attempts.incrementAndGet();
            try {
                while (!closed) {
                    if (XmmsArtworkWorker.PROCESS_LEASE.tryAcquire(50, TimeUnit.MILLISECONDS)) { acquired = true; break; }
                }
                if (closed || !acquired) return;
                int running = active.incrementAndGet(); peakActive.accumulateAndGet(running, Math::max);
                String raw = nativePrepare(filesDir, index);
                if (raw == null || closed) continue;
                key = new JSONObject(raw).getString("key");
                if (XmmsArtworkCache.lookup(application, key + ":512") != null) continue;
                renders.incrementAndGet();
                long pngStartedNs = SystemClock.elapsedRealtimeNanos();
                byte[] png = nativeRender(filesDir, index, raw);
                XmmsArtworkCache.noteNativePng(pngStartedNs, png);
                if (png == null || closed || png.length > 1024 * 1024
                        || !raw.equals(nativeSnapshot(filesDir, index))) continue;
                JSONObject value = new JSONObject(raw);
                XmmsArtworkCache.storePng(application, value.getString("key") + ":512", png);
                if (closed || !raw.equals(nativeSnapshot(filesDir, index))) continue;
                published = true;
                synchronized (lock) {
                    if (closed) continue;
                    main.removeCallbacksAndMessages(this);
                    main.postAtTime(() -> { if (!closed && listener != null) listener.changed(); }, this, SystemClock.uptimeMillis());
                }
            } catch (InterruptedException stop) { Thread.currentThread().interrupt(); return; }
            catch (JSONException | java.io.IOException | OutOfMemoryError unavailable) { /* app icon fallback */ }
            finally {
                if (!published && key != null && !closed) {
                    if (failedUntil.size() >= 32) failedUntil.clear();
                    failedUntil.put(key, SystemClock.uptimeMillis() + 5000);
                }
                if (closed) cancelled.incrementAndGet();
                if (acquired) { active.set(0); XmmsArtworkWorker.PROCESS_LEASE.release(); }
            }
        }
    }

    JSONObject metrics() {
        JSONObject value = new JSONObject();
        try {
            value.put("attempts", attempts.get()).put("renders", renders.get()).put("cancelled", cancelled.get())
                .put("coalesced", coalesced.get()).put("peakActive", peakActive.get()).put("closed", closed);
            synchronized (lock) { value.put("pending", pending >= 0 ? 1 : 0); }
        } catch (JSONException impossible) { throw new IllegalStateException(impossible); }
        return value;
    }

    void close() {
        synchronized (lock) {
            closed = true; pending = -1; main.removeCallbacksAndMessages(this);
        }
        worker.shutdownNow(); listener = null;
    }
}
