package org.xmms.artworkclient;

import android.app.Activity;
import android.content.ComponentName;
import android.media.MediaMetadata;
import android.net.Uri;
import android.graphics.BitmapFactory;
import android.os.ParcelFileDescriptor;
import android.media.browse.MediaBrowser;
import android.media.session.MediaController;
import android.media.session.PlaybackState;
import android.graphics.Bitmap;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.util.Log;
import org.json.JSONObject;
import org.json.JSONException;

/** Separate-UID test client, never packaged into the player APK. */
public final class ArtworkClientActivity extends Activity {
    private static final String TAG = "XMMS_EXTERNAL_ARTWORK";
    private final Handler handler = new Handler(Looper.getMainLooper());
    private MediaBrowser browser;
    private MediaController controller;
    private int debugEvents;
    private int browseCount;
    private int browseUris;
    private int browseBitmaps;
    private long browseStarted;
    private long browseLatencyMs;
    private String beforeDebugTitle;
    private String beforeDebugMediaId;
    private int beforeDebugState;
    private boolean challenged;
    private JSONObject workerStats;
    private boolean auditedProvider;
    private long auditRenders;
    private long auditWrites;

    private Bundle hints() {
        Bundle hints = new Bundle();
        String mode = getIntent().getStringExtra("mode");
        hints.putBoolean("xmms.artwork.high_resolution", "high-resolution".equals(mode) || "browse-unpaged".equals(mode));
        return hints;
    }

    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        browser = new MediaBrowser(this,
                new ComponentName("org.xmms.renascene", "org.xmms.renascene.XmmsPlaybackService"),
                new MediaBrowser.ConnectionCallback() {
                    @Override public void onConnected() {
                        controller = new MediaController(ArtworkClientActivity.this, browser.getSessionToken());
                        controller.registerCallback(callback, handler);
                        String mode = getIntent().getStringExtra("mode");
                        if ("read-uri".equals(mode)) {
                            JSONObject report = new JSONObject();
                            try {
                                Uri uri = Uri.parse(getIntent().getStringExtra("uri"));
                                report.put("readRejected", rejects(uri, "r")).put("clientUid", android.os.Process.myUid());
                                Log.i(TAG, "result " + report);
                            } catch (JSONException invalid) { Log.e(TAG, "invalid-report"); }
                            finish();
                        } else if ("debug-denial".equals(mode)) {
                            for (String command : new String[]{"run", "reverse", "rapid-skip", "stale", "replace", "configure", "reorder-replay"}) {
                                Bundle input = new Bundle(); input.putString("command", command); input.putInt("count", 100);
                                input.putBoolean("trace", true);
                                controller.getTransportControls().sendCustomAction("xmms-debug-artwork-worker", input);
                            }
                            handler.postDelayed(ArtworkClientActivity.this::report, 700);
                        } else {
                            if ("high-resolution".equals(mode) || "browse-unpaged".equals(mode)) {
                                Bundle page = new Bundle(); page.putInt(MediaBrowser.EXTRA_PAGE, 0); page.putInt(MediaBrowser.EXTRA_PAGE_SIZE, 2);
                                browseStarted = android.os.SystemClock.elapsedRealtime();
                                MediaBrowser.SubscriptionCallback subscription = new MediaBrowser.SubscriptionCallback() {
                                    @Override public void onChildrenLoaded(String id, java.util.List<MediaBrowser.MediaItem> items) { loaded(items); }
                                    @Override public void onChildrenLoaded(String id, java.util.List<MediaBrowser.MediaItem> items, Bundle options) { loaded(items); }
                                    private void loaded(java.util.List<MediaBrowser.MediaItem> items) {
                                        browseCount = items.size(); browseUris = 0; browseBitmaps = 0;
                                        browseLatencyMs = android.os.SystemClock.elapsedRealtime() - browseStarted;
                                        for (MediaBrowser.MediaItem item : items) {
                                            if (item.getDescription().getIconBitmap() != null) browseBitmaps++;
                                            Uri uri = item.getDescription().getIconUri();
                                            if (uri != null && "content".equals(uri.getScheme())) browseUris++;
                                        }
                                    }
                                };
                                if ("browse-unpaged".equals(mode)) browser.subscribe("xmms-playlist", subscription);
                                else browser.subscribe("xmms-playlist", page, subscription);
                            }
                            if (!"browse-unpaged".equals(mode)) controller.getTransportControls().playFromMediaId("track:0", null);
                            if (getIntent().getBooleanExtra("stats", false)) handler.postDelayed(() -> {
                                Bundle request = new Bundle(); request.putString("command", "stats");
                                controller.getTransportControls().sendCustomAction("xmms-debug-artwork-worker", request);
                            }, 2200);
                            handler.postDelayed(ArtworkClientActivity.this::report, 2500);
                        }
                    }
                    @Override public void onConnectionFailed() { Log.e(TAG, "connection-failed"); finish(); }
                }, hints());
        browser.connect();
    }

    private final MediaController.Callback callback = new MediaController.Callback() {
        @Override public void onSessionEvent(String event, Bundle value) {
            if ("xmms-artwork-worker".equals(event)) {
                debugEvents++;
                try { workerStats = new JSONObject(value.getString("stats")); }
                catch (JSONException invalid) { Log.e(TAG, "invalid-worker-stats"); }
            }
        }
    };

    private boolean rejects(Uri uri, String mode) {
        try (ParcelFileDescriptor descriptor = getContentResolver().openFileDescriptor(uri, mode)) {
            return false;
        } catch (java.io.IOException | SecurityException | IllegalArgumentException expected) { return true; }
    }

    private void security(JSONObject report, Uri uri) throws JSONException {
        String base = "content://org.xmms.renascene.artwork/artwork/";
        int rejected = 0;
        for (String value : new String[]{base + "00000000000000000000000000000000", base + "../config", base + "%2e%2e%2ffiles%2fconfig",
                base + "%252e%252e", base + uri.getLastPathSegment() + "/extra", "content://org.xmms.renascene.artwork/files/config"}) {
            if (rejects(Uri.parse(value), "r")) rejected++;
        }
        report.put("invalidReadsRejected", rejected);
        int modes = 0;
        for (String mode : new String[]{"w", "rw", "wa", "rwt", "wt"}) if (rejects(uri, mode)) modes++;
        report.put("writeModesRejected", modes);
        int mutations = 0;
        try { getContentResolver().insert(uri, new android.content.ContentValues()); }
        catch (UnsupportedOperationException | SecurityException expected) { mutations++; }
        try { getContentResolver().update(uri, new android.content.ContentValues(), null, null); }
        catch (UnsupportedOperationException | SecurityException expected) { mutations++; }
        try { getContentResolver().delete(uri, null, null); }
        catch (UnsupportedOperationException | SecurityException expected) { mutations++; }
        try (android.database.Cursor ignored = getContentResolver().query(uri, null, null, null, null)) {}
        catch (UnsupportedOperationException | SecurityException expected) { mutations++; }
        report.put("mutationsRejected", mutations);
        boolean mimeRejected = false;
        try (android.content.res.AssetFileDescriptor ignored = getContentResolver().openTypedAssetFileDescriptor(uri, "text/plain", null)) {}
        catch (java.io.IOException expected) { mimeRejected = true; }
        report.put("unsupportedMimeRejected", mimeRejected);
        Bundle options = new Bundle(); options.putSize(android.content.ContentResolver.EXTRA_SIZE, new android.util.Size(160, 160));
        try (android.content.res.AssetFileDescriptor descriptor = getContentResolver().openTypedAssetFileDescriptor(uri, "image/png", options)) {
            Bitmap image = BitmapFactory.decodeFileDescriptor(descriptor.getParcelFileDescriptor().getFileDescriptor());
            report.put("typedWidth", image == null ? 0 : image.getWidth());
        } catch (java.io.IOException failed) { report.put("typedFailed", true); }
        options.putSize(android.content.ContentResolver.EXTRA_SIZE, new android.util.Size(100000, 100000));
        boolean oversized = false;
        try (android.content.res.AssetFileDescriptor ignored = getContentResolver().openTypedAssetFileDescriptor(uri, "image/png", options)) {}
        catch (java.io.IOException | IllegalArgumentException expected) { oversized = true; }
        report.put("oversizedRejected", oversized);
    }

    private void report() {
        if (getIntent().getBooleanExtra("challenge", false) && !challenged) {
            challenged = true;
            MediaMetadata before = controller.getMetadata();
            beforeDebugTitle = before == null ? null : before.getString(MediaMetadata.METADATA_KEY_TITLE);
            beforeDebugMediaId = before == null ? null : before.getString(MediaMetadata.METADATA_KEY_MEDIA_ID);
            beforeDebugState = controller.getPlaybackState() == null ? -1 : controller.getPlaybackState().getState();
            for (String command : new String[]{"run", "reverse", "rapid-skip", "stale", "replace", "configure", "reorder-replay"}) {
                Bundle input = new Bundle(); input.putString("command", command); input.putInt("count", 100);
                controller.getTransportControls().sendCustomAction("xmms-debug-artwork-worker", input);
            }
            handler.postDelayed(this::report, 700);
            return;
        }
        JSONObject report = new JSONObject();
        try {
            MediaMetadata metadata = controller.getMetadata();
            PlaybackState state = controller.getPlaybackState();
            if (workerStats != null) report.put("workerStats", workerStats);
            if (challenged) report.put("debugChallengeUnchanged", metadata != null && state != null
                    && java.util.Objects.equals(beforeDebugTitle, metadata.getString(MediaMetadata.METADATA_KEY_TITLE))
                    && java.util.Objects.equals(beforeDebugMediaId, metadata.getString(MediaMetadata.METADATA_KEY_MEDIA_ID))
                    && beforeDebugState == state.getState());
            report.put("debugEvents", debugEvents).put("clientUid", android.os.Process.myUid())
                .put("state", state == null ? -1 : state.getState())
                .put("queueSize", controller.getQueue() == null ? 0 : controller.getQueue().size())
                .put("browseCount", browseCount).put("browseUris", browseUris).put("browseBitmaps", browseBitmaps)
                .put("browseLatencyMs", browseLatencyMs);
            if (metadata != null) {
                report.put("title", metadata.getString(MediaMetadata.METADATA_KEY_TITLE))
                    .put("mediaId", metadata.getString(MediaMetadata.METADATA_KEY_MEDIA_ID));
                Bitmap image = metadata.getBitmap(MediaMetadata.METADATA_KEY_ALBUM_ART);
                report.put("width", image == null ? 0 : image.getWidth()).put("height", image == null ? 0 : image.getHeight());
                String uriString = metadata.getString(MediaMetadata.METADATA_KEY_ALBUM_ART_URI);
                if (uriString != null) {
                    report.put("uri", uriString).put("allUriKeysMatch",
                            uriString.equals(metadata.getString(MediaMetadata.METADATA_KEY_ART_URI))
                            && uriString.equals(metadata.getString(MediaMetadata.METADATA_KEY_DISPLAY_ICON_URI)))
                        .put("duplicateBitmapFields", metadata.getBitmap(MediaMetadata.METADATA_KEY_ART) != null
                            || metadata.getBitmap(MediaMetadata.METADATA_KEY_DISPLAY_ICON) != null);
                    long openStarted = android.os.SystemClock.elapsedRealtimeNanos();
                    try (ParcelFileDescriptor descriptor = getContentResolver().openFileDescriptor(Uri.parse(uriString), "r")) {
                        long decodeStarted = android.os.SystemClock.elapsedRealtimeNanos();
                        Bitmap master = BitmapFactory.decodeFileDescriptor(descriptor.getFileDescriptor());
                        report.put("uriWidth", master.getWidth()).put("uriHeight", master.getHeight())
                            .put("clientUriOpenNs", decodeStarted - openStarted)
                            .put("clientUriDecodeNs", android.os.SystemClock.elapsedRealtimeNanos() - decodeStarted)
                            .put("clientUriBitmapBytes", master.getAllocationByteCount());
                    } catch (java.io.IOException failure) { report.put("uriReadFailed", true); }
                    boolean writeRejected = false;
                    try (ParcelFileDescriptor ignored = getContentResolver().openFileDescriptor(Uri.parse(uriString), "w")) {
                        report.put("writeUnexpected", true);
                    } catch (java.io.IOException | SecurityException expected) { writeRejected = true; }
                    report.put("writeRejected", writeRejected);
                    security(report, Uri.parse(uriString));
                }
                if (image != null) {
                    int[] row = new int[image.getWidth()]; int checksum = 1;
                    for (int y = 0; y < image.getHeight(); y++) {
                        image.getPixels(row, 0, row.length, 0, y, row.length, 1);
                        for (int pixel : row) checksum = 31 * checksum + pixel;
                    }
                    report.put("checksum", Integer.toHexString(checksum));
                }
            }
            if (getIntent().getBooleanExtra("audit_provider", false) && workerStats != null) {
                if (!auditedProvider) {
                    auditRenders = workerStats.getLong("renders");
                    auditWrites = workerStats.getJSONObject("uriCache").getLong("pngWrites");
                    for (int i = 0; i < 100; i++) {
                        if (!rejects(Uri.parse("content://org.xmms.renascene.artwork/artwork/" + String.format(java.util.Locale.ROOT, "%032x", i)), "r")) {
                            throw new JSONException("Guessed object unexpectedly readable");
                        }
                    }
                    auditedProvider = true;
                    Bundle request = new Bundle(); request.putString("command", "stats");
                    controller.getTransportControls().sendCustomAction("xmms-debug-artwork-worker", request);
                    handler.postDelayed(this::report, 500);
                    return;
                }
                report.put("unknownRequestsQuiet", auditRenders == workerStats.getLong("renders")
                    && auditWrites == workerStats.getJSONObject("uriCache").getLong("pngWrites"));
            }
            Log.i(TAG, "result " + report);
        } catch (JSONException error) { Log.e(TAG, "invalid-report"); }
        if (getIntent().getBooleanExtra("hold", false)) handler.postDelayed(this::finish, 30000);
        else finish();
    }

    @Override public void onDestroy() {
        handler.removeCallbacksAndMessages(null);
        if (controller != null) controller.unregisterCallback(callback);
        if (browser != null) browser.disconnect();
        super.onDestroy();
    }
}
