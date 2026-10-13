package org.xmms.renascene;

import android.app.Activity;
import android.content.ComponentName;
import android.content.Intent;
import android.content.pm.ApplicationInfo;
import android.media.browse.MediaBrowser;
import android.media.MediaMetadata;
import android.graphics.Bitmap;
import android.media.session.MediaController;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;

import org.json.JSONException;
import org.json.JSONObject;
import android.util.Log;

import java.util.List;

public final class XmmsAutoProbeActivity extends Activity {
    private static final String TAG = "XMMS_AUTO_PROBE";
    private static final String ACTION_WIDGET_CONTROL =
            "org.xmms.renascene.e2e.WIDGET_CONTROL";

    private static final String ACTION_ARTWORK_WORKER = "org.xmms.renascene.e2e.ARTWORK_WORKER";
    private MediaBrowser browser;
    private MediaController controller;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private boolean followupSent;
    private boolean transportSettled = true;
    private long callbackNs;
    private int metadataChanges;
    private JSONObject lastWorkerStats;
    private JSONObject firstInspection;
    private boolean exportStarted;
    private Intent handledIntent;
    private final Runnable timeout = () -> { Log.e(TAG, "artwork timeout"); finish(); };

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        handledIntent = getIntent();
        if ((getApplicationInfo().flags & ApplicationInfo.FLAG_DEBUGGABLE) == 0) { finish(); return; }
        if ("org.xmms.renascene.e2e.ARTWORK_CACHE_TEST".equals(getIntent().getAction())) {
            final android.content.Context application = getApplicationContext();
            new Thread(() -> {
                try { Log.i(TAG, "cache-test " + XmmsArtworkCacheTests.run(application)); }
                catch (Exception failure) { Log.e(TAG, "cache-test-failed", failure); }
                handler.post(this::finish);
            }, "artwork-cache-test").start();
            return;
        }
        if (ACTION_WIDGET_CONTROL.equals(getIntent().getAction())) {
            int control = getIntent().getIntExtra("control", XmmsPlaybackService.CONTROL_PLAY);
            sendBroadcast(
                    new Intent(this, XmmsPlayerWidget.class)
                            .setAction("org.xmms.renascene.widget.CONTROL")
                            .putExtra("control", control));
            finish();
            return;
        }
        browser = new MediaBrowser(
                this,
                new ComponentName(this, XmmsPlaybackService.class),
                new MediaBrowser.ConnectionCallback() {
                    @Override
                    public void onConnected() {
                        if (ACTION_ARTWORK_WORKER.equals(getIntent().getAction())) {
                            controller = new MediaController(XmmsAutoProbeActivity.this, browser.getSessionToken());
                            controller.registerCallback(artworkCallback, handler);
                            Bundle request = getIntent().getExtras();
                            if ("observe".equals(getIntent().getStringExtra("command"))) {
                                handler.postDelayed(() -> {
                                    Bundle inspect = new Bundle(); inspect.putString("command", "stats");
                                    controller.getTransportControls().sendCustomAction("xmms-debug-artwork-worker", inspect);
                                }, 300);
                                return;
                            }
                            String control = getIntent().getStringExtra("transport");
                            if (control != null) {
                                transportSettled = false;
                                if ("pause".equals(control)) controller.getTransportControls().pause();
                                else if ("resume".equals(control)) controller.getTransportControls().play();
                                else if ("next".equals(control)) controller.getTransportControls().skipToNext();
                                else if ("previous".equals(control)) controller.getTransportControls().skipToPrevious();
                                else if ("seek".equals(control)) controller.getTransportControls().seekTo(getIntent().getLongExtra("seek_ms", 5000));
                                else if ("stop".equals(control)) controller.getTransportControls().stop();
                                else if ("play-id".equals(control)) controller.getTransportControls().playFromMediaId(getIntent().getStringExtra("media_id"), null);
                                handler.postDelayed(() -> {
                                    transportSettled = true;
                                    Bundle inspect = new Bundle(); inspect.putString("command", "stats");
                                    controller.getTransportControls().sendCustomAction("xmms-debug-artwork-worker", inspect);
                                }, 400);
                            } else controller.getTransportControls().sendCustomAction("xmms-debug-artwork-worker", request == null ? Bundle.EMPTY : request);
                            int timeoutMs = "timed-skip".equals(getIntent().getStringExtra("command")) ? 120000 : 8000;
                            handler.postDelayed(timeout, timeoutMs);
                            return;
                        }
                        String root = browser.getRoot();
                        Log.i(TAG, "connected root=" + root);
                        browser.subscribe(root, subscriptionCallback);
                    }

                    @Override
                    public void onConnectionFailed() {
                        Log.e(TAG, "connection failed");
                        finish();
                    }
                },
                null);
        browser.connect();
    }

    @Override
    protected void onResume() {
        super.onResume();
        if (handledIntent != null && getIntent() != handledIntent) recreate();
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        recreate();
    }

    @Override
    protected void onDestroy() {
        handler.removeCallbacksAndMessages(null);
        if (controller != null) controller.unregisterCallback(artworkCallback);
        if (browser != null && browser.isConnected()) {
            browser.disconnect();
        }
        super.onDestroy();
    }

    private boolean controlSatisfied() {
        String expected = getIntent().getStringExtra("expected_media_id");
        if (expected == null) return true;
        MediaMetadata metadata = controller == null ? null : controller.getMetadata();
        return metadata != null && expected.equals(metadata.getString(MediaMetadata.METADATA_KEY_MEDIA_ID));
    }

    private void inspectArtwork() {
        long inspectionStartedNs = SystemClock.elapsedRealtimeNanos();
        if (controller == null) return;
        MediaMetadata metadata = controller.getMetadata();
        if (metadata == null) return;
        Bitmap bitmap = metadata.getBitmap(MediaMetadata.METADATA_KEY_ALBUM_ART);
        JSONObject result = new JSONObject();
        try {
            result.put("probeToken", getIntent().getStringExtra("probe_token"))
                .put("title", metadata.getString(MediaMetadata.METADATA_KEY_TITLE))
                .put("mediaId", metadata.getString(MediaMetadata.METADATA_KEY_MEDIA_ID))
                .put("width", bitmap == null ? 0 : bitmap.getWidth()).put("height", bitmap == null ? 0 : bitmap.getHeight());
            if (bitmap != null) {
                int[] row = new int[bitmap.getWidth()];
                int hash = 1;
                int redMax = 0, greenMax = 0, blueMax = 0;
                boolean opaque = true;
                for (int y = 0; y < bitmap.getHeight(); y++) {
                    bitmap.getPixels(row, 0, row.length, 0, y, row.length, 1);
                    for (int pixel : row) {
                        hash = 31 * hash + pixel; opaque &= (pixel >>> 24) == 255;
                        redMax = Math.max(redMax, (pixel >>> 16) & 255);
                        greenMax = Math.max(greenMax, (pixel >>> 8) & 255);
                        blueMax = Math.max(blueMax, pixel & 255);
                    }
                }
                result.put("checksum", Integer.toHexString(hash)).put("opaque", opaque)
                    .put("redMax", redMax).put("greenMax", greenMax).put("blueMax", blueMax);
            }
            long inspectedNs = SystemClock.elapsedRealtimeNanos();
            result.put("inspectedNs", inspectedNs).put("inspectionNs", inspectedNs - inspectionStartedNs)
                .put("callbackNs", callbackNs).put("callbackToInspectNs", callbackNs == 0 ? 0 : inspectedNs - callbackNs)
                .put("metadataChanges", metadataChanges);
            if (lastWorkerStats != null && lastWorkerStats.optString("checksum").equals(result.optString("checksum"))
                    && lastWorkerStats.optLong("requestNs") > 0) {
                long readyNs = inspectedNs;
                if (firstInspection != null && firstInspection.optString("checksum").equals(result.optString("checksum"))
                        && firstInspection.optLong("inspectedNs") >= lastWorkerStats.getLong("requestNs")) {
                    readyNs = Math.min(readyNs, firstInspection.getLong("inspectedNs"));
                }
                result.put("requestToClientReadyNs", readyNs - lastWorkerStats.getLong("requestNs"))
                    .put("firstClientReadyNs", readyNs).put("requestTag", lastWorkerStats.optString("requestTag"));
            }
            if (bitmap != null && getIntent().getBooleanExtra("export", false) && !exportStarted) {
                exportStarted = true;
                final Bitmap image = bitmap;
                // Export only the known current bitmap to a fixed app-private file.
                // This test-artifact work never runs on the controller/main thread.
                new Thread(() -> {
                    java.io.File output = new java.io.File(getFilesDir(), "artwork-probe.png");
                    try (java.io.FileOutputStream stream = new java.io.FileOutputStream(output)) {
                        image.compress(Bitmap.CompressFormat.PNG, 100, stream);
                        Log.i(TAG, "artwork-export ready");
                    } catch (java.io.IOException error) { Log.e(TAG, "artwork-export failed"); }
                }, "xmms-artwork-probe-export").start();
            }
            if (bitmap != null && (firstInspection == null || !firstInspection.optString("checksum").equals(result.optString("checksum"))
                    || firstInspection.optLong("callbackNs") != callbackNs)) firstInspection = new JSONObject(result.toString());
            Log.i(TAG, "metadata-art " + result);
        } catch (JSONException error) { Log.e(TAG, "art inspection failed"); }
    }

    private boolean artworkPublicationReady(JSONObject stats) {
        // A retained bitmap/status can belong to the previous generation. For
        // live playback also wait for the service's validated metadata publish.
        return stats.optLong("completedNs") >= stats.optLong("requestNs", 1)
                && (stats.optInt("playbackState") == 0 || stats.optBoolean("hasMetadataArt"));
    }

    private final MediaController.Callback artworkCallback = new MediaController.Callback() {
        @Override public void onMetadataChanged(MediaMetadata metadata) {
            callbackNs = SystemClock.elapsedRealtimeNanos(); metadataChanges++;
            inspectArtwork();
            boolean waitingForPublication = lastWorkerStats != null
                    && ("ready".equals(lastWorkerStats.optString("status"))
                        || "cache-hit".equals(lastWorkerStats.optString("status")));
            if ((getIntent().getStringExtra("expected_media_id") != null || waitingForPublication)
                    && transportSettled && controlSatisfied()) {
                Bundle inspect = new Bundle(); inspect.putString("command", "stats");
                controller.getTransportControls().sendCustomAction("xmms-debug-artwork-worker", inspect);
            }
        }
        @Override public void onSessionEvent(String event, Bundle extras) {
            if (!"xmms-artwork-worker".equals(event) || extras == null || !transportSettled || !controlSatisfied()) return;
            String stats = extras.getString("stats", "{}");
            try {
                lastWorkerStats = new JSONObject(stats);
                stats = lastWorkerStats.put("probeToken", getIntent().getStringExtra("probe_token")).toString();
            }
            catch (JSONException invalid) { Log.e(TAG, "invalid worker stats"); return; }
            Log.i(TAG, "artwork " + stats);
            inspectArtwork();
            try {
                JSONObject value = new JSONObject(stats);
                if (value.optBoolean("mutationPending")) return;
                String status = value.optString("status");
                String scenario = getIntent().getStringExtra("case");
                if (!followupSent && (("rendered".equals(status) && ("close".equals(scenario) || "replace".equals(scenario)))
                        || ("ready".equals(status) && ("cache".equals(scenario) || "guard".equals(scenario))))) {
                    followupSent = true;
                    Bundle request = new Bundle();
                    request.putString("command", "cache".equals(scenario) ? "cached" : scenario);
                    controller.getTransportControls().sendCustomAction("xmms-debug-artwork-worker", request);
                    return;
                }
                if ("observe".equals(getIntent().getStringExtra("command"))) return;
                if ((("ready".equals(status) || "cache-hit".equals(status)) && artworkPublicationReady(value))
                        || "closed".equals(status) || "disabled".equals(status) || "snapshot-error".equals(status) || "main-rejected".equals(status)
                        || ("error".equals(status) && value.optInt("failures") >= 3)) finish();
            } catch (JSONException error) { Log.e(TAG, "invalid artwork event"); finish(); }
        }
    };

    private final MediaBrowser.SubscriptionCallback subscriptionCallback =
            new MediaBrowser.SubscriptionCallback() {
                @Override
                public void onChildrenLoaded(
                        String parentId,
                        List<MediaBrowser.MediaItem> children) {
                    Log.i(TAG, "children parent=" + parentId + " count=" + children.size());
                    if ("xmms-root".equals(parentId) && !children.isEmpty()) {
                        browser.subscribe(children.get(0).getMediaId(), this);
                        return;
                    }
                    if ("xmms-playlist".equals(parentId) && !children.isEmpty()) {
                        Log.i(
                                TAG,
                                "first title="
                                        + children.get(0).getDescription().getTitle());
                    }
                    finish();
                }

                @Override
                public void onError(String parentId) {
                    Log.e(TAG, "browse failed parent=" + parentId);
                    finish();
                }
            };
}
