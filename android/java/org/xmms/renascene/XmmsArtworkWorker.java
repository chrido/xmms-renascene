package org.xmms.renascene;

import android.content.Context;
import android.graphics.Bitmap;
import android.net.Uri;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.util.Log;

import org.json.JSONException;
import org.json.JSONObject;

import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.Semaphore;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicLong;

/** Service-owned latest-wins worker. No Activity or native service GlobalRef. */
final class XmmsArtworkWorker {
    private static final String TAG = "XmmsArtwork";
    private static final int SIZE = 320;
    private static final AtomicLong NEXT_EPOCH = new AtomicLong();
    // Cover cold I/O, native buffers AND Java Bitmap creation across replacement
    // services. Acquisition/cancellation waits happen only on worker threads.
    static final Semaphore PROCESS_LEASE = new Semaphore(1, true);
    private static final AtomicInteger ACTIVE = new AtomicInteger();
    private static final AtomicInteger PEAK_ACTIVE = new AtomicInteger();

    static { System.loadLibrary("xmms_renascene"); }
    private static native String nativeSnapshot(String filesDir);
    private static native void nativeSelect(long epoch, long generation);
    private static native void nativeCancel(long epoch);
    private static native int nativeResolveSettings(String filesDir, long epoch, long generation);
    private static native byte[] nativeRenderPng(String filesDir, String snapshot, long epoch, long generation);
    private static native int[] nativeRender(String filesDir, String snapshot, long epoch, long generation, long[] timings, int outputSize);

    interface Listener {
        void onReady(Snapshot snapshot, Bitmap bitmap);
        void onDiagnostic(String status);
    }

    static final class Snapshot {
        final String raw;
        final String identity;
        final String mediaIdentity;
        final String key;
        final String track;
        final boolean resolved;
        final boolean enabled;
        final boolean hasTrack;

        Snapshot(String raw) throws JSONException {
            if (raw == null || raw.length() > 65536) throw new JSONException("invalid snapshot");
            JSONObject value = new JSONObject(raw);
            this.raw = raw;
            identity = value.getString("identity");
            mediaIdentity = value.getString("mediaIdentity");
            key = value.getString("key");
            track = new JSONObject(key).getString("track");
            resolved = value.getBoolean("resolved");
            enabled = value.getBoolean("enabled");
            hasTrack = value.getBoolean("hasTrack");
        }
    }

    private static final class Job {
        final long generation;
        final Snapshot snapshot;
        final long requestedNs;
        Job(long generation, Snapshot snapshot, long requestedNs) {
            this.generation = generation; this.snapshot = snapshot;
            this.requestedNs = requestedNs;
        }
    }

    private static final class Result {
        final Snapshot snapshot;
        final Bitmap bitmap;
        final String checksum;
        final long[] timings;
        final Uri uri;
        Result(Snapshot snapshot, Bitmap bitmap, String checksum, long[] timings, Uri uri) {
            this.snapshot = snapshot; this.bitmap = bitmap; this.checksum = checksum; this.timings = timings; this.uri = uri;
        }
    }

    private final String filesDir;
    private final Context application;
    private volatile boolean highResolution;
    private Uri lastUri;
    private final Handler main;
    private final boolean debug;
    private final Object queueLock = new Object();
    private final long epoch = NEXT_EPOCH.incrementAndGet();
    private final ThreadPoolExecutor executor;
    // Main-thread-only cache and listener; background code never reads these.
    private final LinkedHashMap<String, Result> cache = new LinkedHashMap<>(3, 0.75f, true);
    private Listener listener;
    private Snapshot desired;
    private Job pending;
    private CompletionCallback postedCompletion;
    private boolean draining;
    private volatile long generation;
    private volatile boolean closed;
    private volatile int debugDelayMs;
    private volatile int debugDeliveryDelayMs;
    private volatile int debugColdDelayMs;
    private volatile boolean traceEnabled;
    private final AtomicInteger traceRecords = new AtomicInteger();
    private volatile long lastRequestNs;
    private long notedRequestNs;
    private long completedNs;
    private long[] lastTimings = new long[7];
    private final AtomicInteger debugFailures = new AtomicInteger();
    private final AtomicInteger attempts = new AtomicInteger();
    private final AtomicInteger renders = new AtomicInteger();
    private final AtomicInteger cancellations = new AtomicInteger();
    private final AtomicInteger paletteFallbacks = new AtomicInteger();
    private final AtomicInteger coalesced = new AtomicInteger();
    private final AtomicInteger workerWakeups = new AtomicInteger();
    private int accepted;
    private int stale;
    private int cacheHits;
    private int failures;
    private int peakPending;
    private boolean mainRejected;
    private String lastStatus = "idle";
    private String checksum = "";
    private long lastWarningMs = Long.MIN_VALUE;
    private Bitmap lastBitmap;
    private final Runnable retry = () -> requestInternal(true, false);
    private final Runnable deferredRequest = () -> requestInternal(false, false);

    XmmsArtworkWorker(Context application, String filesDir, Handler main, boolean debug, Listener listener) {
        requireMain();
        if (epoch <= 0) throw new IllegalStateException("artwork epoch exhausted");
        this.application = application.getApplicationContext();
        this.filesDir = filesDir; this.main = main; this.debug = debug; this.listener = listener;
        executor = new ThreadPoolExecutor(1, 1, 0L, TimeUnit.MILLISECONDS,
                new ArrayBlockingQueue<Runnable>(1), runnable -> {
                    Thread thread = new Thread(runnable, "xmms-artwork-" + epoch);
                    thread.setPriority(Thread.MIN_PRIORITY);
                    return thread;
                });
        nativeSelect(epoch, 0);
    }

    Snapshot currentSnapshot() {
        requireMain();
        try { return new Snapshot(nativeSnapshot(filesDir)); }
        catch (JSONException error) { return null; }
    }

    void noteRequest(Snapshot snapshot) {
        requireMain();
        if (snapshot != null && (desired == null || !desired.raw.equals(snapshot.raw)) && notedRequestNs == 0) {
            notedRequestNs = SystemClock.elapsedRealtimeNanos();
        }
    }

    Bitmap cachedBitmap(Snapshot snapshot) {
        requireMain();
        if (snapshot == null || !snapshot.resolved || !snapshot.enabled || !snapshot.hasTrack) return null;
        Result result = cache.get(snapshot.key);
        return result == null ? null : result.bitmap;
    }

    Uri cachedUri(Snapshot snapshot) {
        requireMain();
        if (snapshot == null || !snapshot.enabled) return null;
        Result result = cache.get(snapshot.key);
        return result == null ? null : result.uri;
    }

    // Service state callbacks can be nested within a Rust ordering-lock stack.
    // Always cross the Handler boundary before querying native snapshot state.
    void requestLater() {
        requireMain();
        if (closed) return;
        main.removeCallbacks(deferredRequest);
        main.postAtTime(deferredRequest, this, SystemClock.uptimeMillis());
    }

    private void requestInternal(boolean retrying, boolean force) {
        requireMain();
        if (closed) return;
        final Snapshot snapshot;
        try { snapshot = new Snapshot(nativeSnapshot(filesDir)); }
        catch (JSONException failure) {
            if (generation == Long.MAX_VALUE) { close(); return; }
            generation++; nativeSelect(epoch, generation);
            synchronized (queueLock) { pending = null; clearPostedLocked(); }
            desired = null; lastBitmap = null; lastUri = null;
            if (listener != null) listener.onReady(null, null);
            diagnostic("snapshot-error"); return;
        }
        if (!force && !retrying && desired != null && desired.raw.equals(snapshot.raw)
                && (!highResolution || lastUri != null || !snapshot.enabled)) return;
        if (desired == null || !desired.identity.equals(snapshot.identity)) failures = 0;
        desired = snapshot;
        main.removeCallbacks(retry);
        if (generation == Long.MAX_VALUE) { close(); return; }
        generation++;
        lastRequestNs = notedRequestNs == 0 ? SystemClock.elapsedRealtimeNanos() : notedRequestNs;
        notedRequestNs = 0;
        completedNs = 0; lastTimings = new long[7];
        // Diagnostics describe this generation, not the previous ready bitmap.
        // A stats client must not finish while the replacement is still running.
        lastStatus = "pending";
        nativeSelect(epoch, generation);
        synchronized (queueLock) {
            if (pending != null) coalesced.incrementAndGet();
            pending = null; clearPostedLocked();
        }
        if (snapshot.resolved && (!snapshot.enabled || !snapshot.hasTrack)) {
            lastBitmap = null; lastUri = null; cache.clear(); checksum = "";
            if (listener != null) listener.onReady(snapshot, null);
            diagnostic("disabled");
            return;
        }
        if (!force && snapshot.resolved) {
            Result cached = cache.get(snapshot.key);
            if (cached != null && (!highResolution || cached.uri != null)) {
                cacheHits++; lastBitmap = cached.bitmap; checksum = cached.checksum; lastUri = cached.uri;
                lastTimings = new long[7]; completedNs = SystemClock.elapsedRealtimeNanos();
                if (listener != null) listener.onReady(snapshot, cached.bitmap);
                diagnostic("cache-hit");
                return;
            }
        }
        Job job = new Job(generation, snapshot, lastRequestNs);
        synchronized (queueLock) {
            if (pending != null) coalesced.incrementAndGet();
            pending = job; // replace, do not enqueue one executor task per request
            peakPending = 1;
            if (!draining) {
                draining = true;
                executor.execute(this::drain);
            }
        }
    }

    private boolean cancelled(Job job) { return closed || generation != job.generation; }

    private final class CompletionCallback implements Runnable {
        private final Job job;
        private final Result result;
        private final boolean error;
        CompletionCallback(Job job, Result result, boolean error) {
            this.job = job; this.result = result; this.error = error;
        }
        @Override public void run() {
            synchronized (queueLock) {
                if (postedCompletion != this) return;
                postedCompletion = null;
            }
            complete(job, result, error);
        }
    }

    // queueLock guards result retention as well as pending jobs: at most one
    // completed Bitmap waits for the main looper, even during debug delays.
    private void clearPostedLocked() {
        if (postedCompletion != null) main.removeCallbacks(postedCompletion);
        postedCompletion = null;
    }

    private void drain() {
        workerWakeups.incrementAndGet();
        if (Looper.myLooper() == Looper.getMainLooper()) throw new IllegalStateException("artwork on main");
        for (;;) {
            Job job;
            synchronized (queueLock) {
                job = pending; pending = null;
                if (job == null || closed) { draining = false; return; }
            }
            Result result = null;
            boolean failed = false;
            attempts.incrementAndGet();
            try { result = runWithLease(job); }
            catch (InterruptedException interrupted) {
                Thread.currentThread().interrupt(); cancellations.incrementAndGet(); return;
            } catch (RuntimeException | OutOfMemoryError error) {
                failed = true; // path/snapshot-bearing exception messages are not logged
            }
            synchronized (queueLock) {
                if (cancelled(job)) { cancellations.incrementAndGet(); continue; }
                clearPostedLocked();
                postedCompletion = new CompletionCallback(job, result, failed);
                if (debug && debugDeliveryDelayMs > 0 && result != null && result.bitmap != null) {
                    main.postAtTime(() -> { if (!cancelled(job)) diagnostic("rendered"); }, this, SystemClock.uptimeMillis());
                }
                main.postAtTime(postedCompletion, this,
                        SystemClock.uptimeMillis() + (debug ? debugDeliveryDelayMs : 0));
            }
        }
    }

    private Result runWithLease(Job job) throws InterruptedException {
        boolean acquired = false;
        try {
            while (!cancelled(job)) {
                if (PROCESS_LEASE.tryAcquire(50, TimeUnit.MILLISECONDS)) { acquired = true; break; }
            }
            if (!acquired || cancelled(job)) return null;
            long startedNs = SystemClock.elapsedRealtimeNanos();
            long[] timings = new long[7];
            timings[0] = startedNs - job.requestedNs;
            int active = ACTIVE.incrementAndGet();
            PEAK_ACTIVE.accumulateAndGet(active, Math::max);
            try {
                if (debug) {
                    long until = SystemClock.uptimeMillis() + debugDelayMs;
                    while (SystemClock.uptimeMillis() < until && !cancelled(job)) Thread.sleep(10);
                    if (debugFailures.get() > 0 && debugFailures.getAndDecrement() > 0) throw new IllegalStateException("injected failure");
                }
                if (cancelled(job)) return null;
                long resolveStart = SystemClock.elapsedRealtimeNanos();
                if (!job.snapshot.resolved) {
                    if (debug && debugColdDelayMs > 0) {
                        main.postAtTime(() -> { if (!cancelled(job)) diagnostic("resolving"); }, this, SystemClock.uptimeMillis());
                        long until = SystemClock.uptimeMillis() + debugColdDelayMs;
                        while (SystemClock.uptimeMillis() < until && !cancelled(job)) Thread.sleep(10);
                    }
                    if (cancelled(job)) return null;
                    int resolution = nativeResolveSettings(filesDir, epoch, job.generation);
                    if (resolution == 2) {
                        paletteFallbacks.incrementAndGet();
                        Log.w(TAG, "Artwork palette fallback; saved settings unchanged");
                    }
                    if (resolution == 0) {
                        if (cancelled(job)) return null;
                        throw new IllegalStateException("settings failed");
                    }
                }
                timings[1] = SystemClock.elapsedRealtimeNanos() - resolveStart;
                Snapshot snapshot = new Snapshot(nativeSnapshot(filesDir));
                if (!snapshot.mediaIdentity.equals(job.snapshot.mediaIdentity) || cancelled(job)
                        || (job.snapshot.resolved && !snapshot.raw.equals(job.snapshot.raw))) return null;
                if (!snapshot.enabled || !snapshot.hasTrack) return new Result(snapshot, null, "", timings, null);
                renders.incrementAndGet();
                long[] nativeTimings = new long[2];
                long renderStart = SystemClock.elapsedRealtimeNanos();
                int[] pixels = nativeRender(filesDir, snapshot.raw, epoch, job.generation, nativeTimings, SIZE);
                timings[2] = nativeTimings[0];
                timings[3] = nativeTimings[1];
                timings[4] = SystemClock.elapsedRealtimeNanos() - renderStart;
                if (pixels == null) {
                    if (cancelled(job) || !snapshot.raw.equals(nativeSnapshot(filesDir))) return null;
                    throw new IllegalStateException("render failed");
                }
                if (pixels.length != SIZE * SIZE || cancelled(job)) return null;
                for (int pixel : pixels) if ((pixel >>> 24) != 255) throw new IllegalStateException("nonopaque artwork");
                String digest = Integer.toHexString(Arrays.hashCode(pixels));
                long bitmapStart = SystemClock.elapsedRealtimeNanos();
                Bitmap bitmap = Bitmap.createBitmap(pixels, SIZE, SIZE, Bitmap.Config.ARGB_8888);
                timings[5] = SystemClock.elapsedRealtimeNanos() - bitmapStart;
                if (debug) {
                    // Validate actual Bitmap storage without another image-sized
                    // allocation: reuse the transfer array for the readback.
                    bitmap.getPixels(pixels, 0, SIZE, 0, 0, SIZE, SIZE);
                    if (!digest.equals(Integer.toHexString(Arrays.hashCode(pixels)))) {
                        throw new IllegalStateException("bitmap storage mismatch");
                    }
                }
                Uri uri = null;
                if (highResolution && !cancelled(job)) {
                    long pngStartedNs = SystemClock.elapsedRealtimeNanos();
                    byte[] master = nativeRenderPng(filesDir, snapshot.raw, epoch, job.generation);
                    XmmsArtworkCache.noteNativePng(pngStartedNs, master);
                    if (master == null || cancelled(job)) return null;
                    try { uri = XmmsArtworkCache.storePng(application, snapshot.key + ":512", master); }
                    catch (java.io.IOException unavailable) { /* small bitmap remains a safe fallback */ }
                }
                timings[6] = SystemClock.elapsedRealtimeNanos() - startedNs;
                return cancelled(job) ? null : new Result(snapshot, bitmap, digest, timings, uri);
            } catch (JSONException error) { throw new IllegalStateException("snapshot failed"); }
            finally { ACTIVE.decrementAndGet(); }
        } finally { if (acquired) PROCESS_LEASE.release(); }
    }

    private void complete(Job job, Result result, boolean error) {
        requireMain();
        if (closed || generation != job.generation) { stale++; return; }
        if (result == null && !error) {
            stale++; cancellations.incrementAndGet(); diagnostic("stale");
            desired = null;
            requestLater(); // one latest replacement, not a retry of old inputs
            return;
        }
        if (error) {
            failures++; lastBitmap = null; lastUri = null;
            if (listener != null) listener.onReady(desired, null);
            diagnostic("error");
            if (failures < 3) main.postAtTime(retry, this, SystemClock.uptimeMillis() + (failures == 1 ? 250 : 1000));
            return;
        }
        if (!result.snapshot.raw.equals(nativeSnapshot(filesDir))) {
            stale++; diagnostic("stale"); desired = null; requestLater(); return;
        }
        desired = result.snapshot;
        lastBitmap = result.bitmap; checksum = result.checksum; lastUri = result.uri;
        lastTimings = result.timings; completedNs = SystemClock.elapsedRealtimeNanos();
        if (result.bitmap != null) {
            accepted++;
            cache.put(result.snapshot.key, result);
            while (cache.size() > 2) {
                String oldest = cache.keySet().iterator().next(); cache.remove(oldest);
            }
        }
        if (listener != null) listener.onReady(result.snapshot, result.bitmap);
        diagnostic(result.bitmap == null ? "disabled" : "ready");
    }

    private void diagnostic(String status) {
        lastStatus = status;
        long now = SystemClock.uptimeMillis();
        if (("error".equals(status) || "snapshot-error".equals(status))
                && (lastWarningMs == Long.MIN_VALUE || now - lastWarningMs >= 5000)) {
            lastWarningMs = now;
            Log.w(TAG, "Artwork unavailable; transport unaffected, retries bounded");
        }
        if (debug && traceEnabled && traceRecords.getAndIncrement() < 256) Log.i(TAG, stats().toString());
        if (listener != null) listener.onDiagnostic(status);
    }

    JSONObject stats() {
        requireMain();
        JSONObject value = new JSONObject();
        long cachedBytes = 0;
        for (Result result : cache.values()) cachedBytes += result.bitmap.getAllocationByteCount();
        long postedBytes;
        synchronized (queueLock) {
            Result result = postedCompletion == null ? null : postedCompletion.result;
            postedBytes = result == null || result.bitmap == null ? 0 : result.bitmap.getAllocationByteCount();
        }
        try {
            value.put("uriCache", XmmsArtworkCache.metrics());
            value.put("ownedBitmapBytes", cachedBytes + postedBytes).put("postedBitmapBytes", postedBytes)
                .put("coalesced", coalesced.get()).put("workerWakeups", workerWakeups.get())
                .put("imageBufferReservationBytes", SIZE * SIZE * 4L * 5 + SIZE * 4L * 3
                    + (highResolution ? 2L * 1024 * 1024 + 8192 : 0))
                .put("imageBufferNote", "reservation upper bound; excludes codec/allocator/framework overhead, measure PSS separately");
            value.put("epoch", epoch).put("generation", generation).put("status", lastStatus)
                .put("attempts", attempts.get()).put("renders", renders.get()).put("accepted", accepted)
                .put("stale", stale).put("cancelled", cancellations.get()).put("cacheHits", cacheHits).put("paletteFallbacks", paletteFallbacks.get())
                .put("failures", failures).put("peakPending", peakPending).put("peakActive", PEAK_ACTIVE.get())
                .put("cacheEntries", cache.size()).put("closed", closed).put("checksum", checksum).put("mainRejected", mainRejected)
                .put("width", lastBitmap == null ? 0 : lastBitmap.getWidth())
                .put("height", lastBitmap == null ? 0 : lastBitmap.getHeight())
                .put("requestNs", lastRequestNs).put("completedNs", completedNs)
                .put("queueNs", lastTimings[0]).put("resolveNs", lastTimings[1])
                .put("nativeRenderNs", lastTimings[2]).put("arrayTransferNs", lastTimings[3])
                .put("renderCallNs", lastTimings[4]).put("bitmapNs", lastTimings[5]).put("workerNs", lastTimings[6])
                .put("requestTag", epoch + ":" + generation).put("imageTag", checksum)
                .put("highResolution", highResolution).put("artworkUri", lastUri == null ? "" : lastUri.toString());
        } catch (JSONException impossible) { throw new IllegalStateException(impossible); }
        return value;
    }

    void recoverUri() {
        requireMain();
        if (closed || !highResolution) return;
        cache.clear(); desired = null; lastUri = null; requestLater();
    }

    void enableHighResolution() {
        requireMain();
        if (!closed && !highResolution) { highResolution = true; requestLater(); }
    }

    Uri currentUri() { requireMain(); return lastUri; }

    void debugMainThreadGuard() {
        requireMain();
        if (!debug || closed || desired == null || !desired.resolved) return;
        mainRejected = nativeResolveSettings(filesDir, epoch, generation) == 0
                && nativeRender(filesDir, desired.raw, epoch, generation, new long[2], SIZE) == null;
        diagnostic("main-rejected");
    }

    void debugCachedRequest() {
        requireMain();
        if (!debug || closed) return;
        desired = null;
        requestInternal(false, false);
    }

    void debugSetRenderDelay(int delayMillis) {
        requireMain();
        if (debug && !closed) debugDelayMs = Math.max(0, Math.min(2000, delayMillis));
    }

    void debugConfigure(boolean trace, int coldDelayMillis) {
        requireMain();
        if (!debug || closed) return;
        traceEnabled = trace;
        debugColdDelayMs = Math.max(0, Math.min(2000, coldDelayMillis));
    }

    void debugRequest(int delayMillis, int failCount, int deliveryDelayMillis) {
        requireMain();
        if (!debug || closed) return;
        debugDelayMs = Math.max(0, Math.min(2000, delayMillis));
        debugDeliveryDelayMs = Math.max(0, Math.min(2000, deliveryDelayMillis));
        debugFailures.set(Math.max(0, Math.min(3, failCount)));
        cache.clear(); desired = null;
        requestInternal(false, true);
    }

    void close() {
        requireMain();
        if (closed) return;
        closed = true;
        if (generation < Long.MAX_VALUE) generation++;
        nativeCancel(epoch);
        synchronized (queueLock) { pending = null; clearPostedLocked(); }
        main.removeCallbacksAndMessages(this);
        executor.shutdownNow();
        cache.clear(); lastBitmap = null; lastUri = null; desired = null;
        listener = null; // running closures cannot retain the owning service
        lastStatus = "closed";
        if (debug && traceEnabled && traceRecords.getAndIncrement() < 256) Log.i(TAG, stats().toString());
    }

    boolean isClosed() { return closed; }

    private static void requireMain() {
        if (Looper.myLooper() != Looper.getMainLooper()) throw new IllegalStateException("artwork owner off main");
    }
}
