package org.xmms.renascene;

import android.Manifest;
import android.app.NativeActivity;
import android.content.Context;
import android.content.ClipData;
import android.content.Intent;
import android.content.res.Configuration;
import android.content.pm.PackageManager;
import android.database.ContentObserver;
import android.database.Cursor;
import android.media.AudioManager;
import android.net.Uri;
import android.os.Build;
import android.os.Handler;
import android.os.Looper;
import android.provider.DocumentsContract;
import android.provider.OpenableColumns;
import android.provider.Settings;
import android.view.DisplayCutout;
import android.view.RoundedCorner;
import android.view.WindowInsets;

import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.atomic.AtomicReference;

/**
 * Android main-thread owner for Activity lifecycle, SAF, geometry, and volume observation.
 *
 * <p>Native callbacks from this class are one-way adapters: they validate/convert and enqueue
 * typed Rust events (or request repaint), but never execute domain or playback mutations.
 * Activity-dependent methods may fail after teardown; background playback remains owned by
 * {@link XmmsPlaybackService}.
 */
public final class XmmsActivity extends NativeActivity {
    private static final Handler MAIN_HANDLER = new Handler(Looper.getMainLooper());
    private static final ExecutorService DOCUMENT_EXECUTOR =
            Executors.newSingleThreadExecutor();
    private static final int READ_FLAGS =
            Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION;
    static final String ACTION_PAUSE_PLAYBACK =
            "org.xmms.renascene.action.PAUSE_PLAYBACK";
    static final String ACTION_RESUME_PLAYBACK =
            "org.xmms.renascene.action.RESUME_PLAYBACK";
    static final String ACTION_NEXT_TRACK =
            "org.xmms.renascene.action.NEXT_TRACK";
    static {
        System.loadLibrary("xmms_renascene");
    }

    private native void nativeOnDocumentsSelected(
            int requestCode, long operationId, String[] paths, String error);
    private native void nativeOnActivityResumed();
    private native void nativeOnActivityPaused();
    private native void nativeOnActivityDestroyed();
    private native void nativeOnMediaControl(int control);
    private native void nativeOnMediaVolumeChanged(int volumePercent);
    private native void nativeRequestRepaint();

    private static final class SafeInsetSnapshot {
        static final SafeInsetSnapshot EMPTY =
                new SafeInsetSnapshot(0, 0, 0, 0, 0, 0, 0, 0);

        final int left;
        final int top;
        final int right;
        final int bottom;
        final int width;
        final int height;
        final int orientation;
        final long configGeneration;

        SafeInsetSnapshot(
                int left,
                int top,
                int right,
                int bottom,
                int width,
                int height,
                int orientation,
                long configGeneration) {
            this.left = left;
            this.top = top;
            this.right = right;
            this.bottom = bottom;
            this.width = width;
            this.height = height;
            this.orientation = orientation;
            this.configGeneration = configGeneration;
        }

        boolean hasSameLayout(
                int left,
                int top,
                int right,
                int bottom,
                int width,
                int height,
                int orientation,
                long configGeneration) {
            return this.left == left
                    && this.top == top
                    && this.right == right
                    && this.bottom == bottom
                    && this.width == width
                    && this.height == height
                    && this.orientation == orientation
                    && this.configGeneration == configGeneration;
        }
    }

    private enum MediaControl {
        PAUSE(1),
        RESUME(2),
        NEXT(3);

        final int jniCode;

        MediaControl(int jniCode) {
            this.jniCode = jniCode;
        }
    }

    private static final class MediaControlDispatch {
        // Main-thread transition table:
        // IDLE + enqueue -> IDLE; IDLE + take(ready) -> IN_FLIGHT;
        // IN_FLIGHT + enqueue/take -> IN_FLIGHT; IN_FLIGHT + complete -> IDLE.
        private enum Phase {
            IDLE,
            IN_FLIGHT
        }

        private final ArrayDeque<MediaControl> queued = new ArrayDeque<>();
        private Phase phase = Phase.IDLE;

        void enqueue(MediaControl control) {
            queued.addLast(control);
        }

        MediaControl take(boolean ready) {
            if (!ready || phase == Phase.IN_FLIGHT) {
                return null;
            }
            MediaControl control = queued.pollFirst();
            if (control != null) {
                phase = Phase.IN_FLIGHT;
            }
            return control;
        }

        boolean complete() {
            if (phase != Phase.IN_FLIGHT) {
                return false;
            }
            phase = Phase.IDLE;
            return queued.isEmpty();
        }
    }

    private AudioManager audioManager;
    private final MediaControlDispatch mediaControls = new MediaControlDispatch();
    private volatile boolean activityResumed;
    private boolean nativeLoopReady;
    private boolean mediaVolumeObserverRegistered;
    private int lastReportedMediaVolumePercent = -1;
    // Null when no app-initiated volume change is in flight; non-null holds the clamped
    // percent value set by setMediaVolumePercent() until the observer confirms it.
    private final AtomicReference<Integer> pendingAppMediaVolumePercent = new AtomicReference<>();
    private final ContentObserver mediaVolumeObserver = new ContentObserver(MAIN_HANDLER) {
        @Override
        public void onChange(boolean selfChange) {
            onObservedMediaVolumeChanged();
        }

        @Override
        public void onChange(boolean selfChange, Uri uri) {
            onObservedMediaVolumeChanged();
        }
    };
    private final Object geometryLock = new Object();
    private long configGeneration = 1;
    private volatile SafeInsetSnapshot safeInsetSnapshot = SafeInsetSnapshot.EMPTY;
    private byte[] pendingDocumentContents;
    private final HashMap<Integer, Long> pickerOperationIds = new HashMap<>();

    @Override
    protected void onCreate(android.os.Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        if (Build.VERSION.SDK_INT >= 28) {
            android.view.WindowManager.LayoutParams attributes = getWindow().getAttributes();
            attributes.layoutInDisplayCutoutMode = Build.VERSION.SDK_INT >= 30
                    ? android.view.WindowManager.LayoutParams
                            .LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS
                    : android.view.WindowManager.LayoutParams
                            .LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
            getWindow().setAttributes(attributes);
        }
        setVolumeControlStream(AudioManager.STREAM_MUSIC);
        audioManager = (AudioManager) getSystemService(Context.AUDIO_SERVICE);
        if (Build.VERSION.SDK_INT >= 33
                && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS)
                        != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(new String[] {Manifest.permission.POST_NOTIFICATIONS}, 200);
        }
        getWindow().getDecorView().setOnApplyWindowInsetsListener((view, insets) -> {
            updateSafeInsets(view, insets);
            return insets;
        });
        getWindow().getDecorView().requestApplyInsets();
        handleMediaControlIntent(getIntent());
    }

    @Override
    protected void onResume() {
        super.onResume();
        activityResumed = true;
        nativeLoopReady = true;
        nativeOnActivityResumed();
        registerMediaVolumeObserver();
        getWindow().getDecorView().post(this::dispatchPendingMediaControl);
    }

    @Override
    protected void onPause() {
        unregisterMediaVolumeObserver();
        nativeLoopReady = false;
        activityResumed = false;
        nativeOnActivityPaused();
        super.onPause();
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus && activityResumed) {
            nativeLoopReady = true;
            getWindow().getDecorView().post(this::dispatchPendingMediaControl);
        }
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        handleMediaControlIntent(intent);
    }

    @Override
    public void onConfigurationChanged(Configuration newConfig) {
        super.onConfigurationChanged(newConfig);
        synchronized (geometryLock) {
            configGeneration++;
        }
        getWindow().getDecorView().requestApplyInsets();
        nativeRequestRepaint();
    }

    @Override
    protected void onDestroy() {
        activityResumed = false;
        nativeLoopReady = false;
        nativeOnActivityDestroyed();
        super.onDestroy();
    }

    public boolean isNativeActivityResumed() {
        return activityResumed;
    }

    private void dispatchMediaControl(int control) {
        nativeOnMediaControl(control);
        getWindow().getDecorView().postInvalidateOnAnimation();
    }

    private void dispatchPendingMediaControl() {
        MediaControl control = mediaControls.take(nativeLoopReady);
        if (control == null) {
            return;
        }
        dispatchMediaControl(control.jniCode);
    }

    private boolean handleMediaControlIntent(Intent intent) {
        if (intent == null) {
            return false;
        }
        if (ACTION_PAUSE_PLAYBACK.equals(intent.getAction())) {
            mediaControls.enqueue(MediaControl.PAUSE);
            setIntent(new Intent(this, XmmsActivity.class));
            getWindow().getDecorView().post(this::dispatchPendingMediaControl);
            return true;
        }
        if (ACTION_RESUME_PLAYBACK.equals(intent.getAction())) {
            mediaControls.enqueue(MediaControl.RESUME);
            setIntent(new Intent(this, XmmsActivity.class));
            getWindow().getDecorView().post(this::dispatchPendingMediaControl);
            return true;
        }
        if (ACTION_NEXT_TRACK.equals(intent.getAction())) {
            mediaControls.enqueue(MediaControl.NEXT);
            setIntent(new Intent(this, XmmsActivity.class));
            getWindow().getDecorView().post(this::dispatchPendingMediaControl);
            return true;
        }
        return false;
    }

    public void completeMediaControl() {
        runOnUiThread(() -> {
            if (mediaControls.complete()) {
                nativeLoopReady = false;
                moveTaskToBack(true);
            } else {
                dispatchPendingMediaControl();
            }
        });
    }

    public void updatePlaybackNotification(
            int state,
            String title,
            int bitrate,
            int frequency,
            int channels,
            long durationMs,
            long positionMs,
            long currentIndex,
            int playlistSize,
            boolean hasPrevious,
            boolean hasNext) {
        runOnUiThread(() -> {
            Intent intent = new Intent(this, XmmsPlaybackService.class)
                    .setAction(XmmsPlaybackService.ACTION_UPDATE)
                    .putExtra(XmmsPlaybackService.EXTRA_STATE, state)
                    .putExtra(XmmsPlaybackService.EXTRA_TITLE, title)
                    .putExtra(XmmsPlaybackService.EXTRA_BITRATE, bitrate)
                    .putExtra(XmmsPlaybackService.EXTRA_FREQUENCY, frequency)
                    .putExtra(XmmsPlaybackService.EXTRA_CHANNELS, channels)
                    .putExtra(XmmsPlaybackService.EXTRA_DURATION_MS, durationMs)
                    .putExtra(XmmsPlaybackService.EXTRA_POSITION_MS, positionMs)
                    .putExtra(XmmsPlaybackService.EXTRA_CURRENT_INDEX, currentIndex)
                    .putExtra(XmmsPlaybackService.EXTRA_PLAYLIST_SIZE, playlistSize)
                    .putExtra(XmmsPlaybackService.EXTRA_HAS_PREVIOUS, hasPrevious)
                    .putExtra(XmmsPlaybackService.EXTRA_HAS_NEXT, hasNext);
            if (state == 0) {
                XmmsPlayerInfoWidget.updateAll(this, state, title, 0, 0, 0);
                stopService(intent);
            } else if (Build.VERSION.SDK_INT >= 26) {
                startForegroundService(intent);
            } else {
                startService(intent);
            }
        });
    }

    private void registerMediaVolumeObserver() {
        if (mediaVolumeObserverRegistered) {
            return;
        }
        getContentResolver().registerContentObserver(
                Settings.System.CONTENT_URI, true, mediaVolumeObserver);
        mediaVolumeObserverRegistered = true;
        reportMediaVolumeIfChanged();
    }

    private void unregisterMediaVolumeObserver() {
        if (!mediaVolumeObserverRegistered) {
            return;
        }
        mediaVolumeObserverRegistered = false;
        try {
            getContentResolver().unregisterContentObserver(mediaVolumeObserver);
        } catch (IllegalArgumentException | IllegalStateException ignored) {
            // The observer is already detached; keep lifecycle teardown idempotent.
        }
    }

    private void onObservedMediaVolumeChanged() {
        if (mediaVolumeObserverRegistered) {
            reportMediaVolumeIfChanged();
        }
    }

    private void reportMediaVolumeIfChanged() {
        if (Looper.myLooper() != Looper.getMainLooper()) {
            MAIN_HANDLER.post(this::reportMediaVolumeIfChanged);
            return;
        }
        int volumePercent = getMediaVolumePercent();
        if (volumePercent < 0 || volumePercent > 100) {
            return;
        }
        Integer requestedPercent = pendingAppMediaVolumePercent.getAndSet(null);
        if (requestedPercent != null && requestedPercent == volumePercent) {
            lastReportedMediaVolumePercent = volumePercent;
            return;
        }
        if (requestedPercent == null && volumePercent == lastReportedMediaVolumePercent) {
            return;
        }
        lastReportedMediaVolumePercent = volumePercent;
        nativeOnMediaVolumeChanged(volumePercent);
    }

    public boolean setMediaVolumePercent(int volumePercent) {
        if (audioManager == null) {
            return false;
        }
        int maxVolume = audioManager.getStreamMaxVolume(AudioManager.STREAM_MUSIC);
        if (maxVolume <= 0) {
            return false;
        }
        int clampedPercent = Math.max(0, Math.min(100, volumePercent));
        int streamVolume = (int) Math.round(maxVolume * (clampedPercent / 100.0));
        audioManager.setStreamVolume(AudioManager.STREAM_MUSIC, streamVolume, 0);
        pendingAppMediaVolumePercent.set(clampedPercent);
        reportMediaVolumeIfChanged();
        return true;
    }

    public int getMediaVolumePercent() {
        if (audioManager == null) {
            return -1;
        }
        int maxVolume = audioManager.getStreamMaxVolume(AudioManager.STREAM_MUSIC);
        if (maxVolume <= 0) {
            return -1;
        }
        int currentVolume = audioManager.getStreamVolume(AudioManager.STREAM_MUSIC);
        int clampedVolume = Math.max(0, Math.min(maxVolume, currentVolume));
        return (int) Math.round(100.0 * clampedVolume / maxVolume);
    }

    public void refreshPlayerWidgets() {
        Context applicationContext = getApplicationContext();
        MAIN_HANDLER.post(() -> {
            XmmsPlayerWidget.refreshAll(applicationContext);
            XmmsPlayerInfoWidget.refreshAll(applicationContext);
        });
    }

    public void openDocuments(
            int requestCode, long operationId, String mimeType, boolean multiple) {
        runOnUiThread(() -> {
            pickerOperationIds.put(requestCode, operationId);
            Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            intent.setType(mimeType);
            intent.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, multiple);
            intent.addFlags(READ_FLAGS);
            startActivityForResult(intent, requestCode);
        });
    }

    public void openDirectory(int requestCode, long operationId) {
        runOnUiThread(() -> {
            pickerOperationIds.put(requestCode, operationId);
            Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
            intent.addFlags(READ_FLAGS);
            startActivityForResult(intent, requestCode);
        });
    }

    public void createDocument(
            int requestCode, long operationId, String mimeType, String title, byte[] contents) {
        runOnUiThread(() -> {
            pickerOperationIds.put(requestCode, operationId);
            pendingDocumentContents = contents;
            Intent intent = new Intent(Intent.ACTION_CREATE_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            intent.setType(mimeType);
            intent.putExtra(Intent.EXTRA_TITLE, title);
            intent.addFlags(Intent.FLAG_GRANT_WRITE_URI_PERMISSION);
            startActivityForResult(intent, requestCode);
        });
    }

    private android.graphics.Rect currentWindowBounds() {
        android.graphics.Rect bounds;
        if (Build.VERSION.SDK_INT >= 30) {
            bounds = getWindowManager().getCurrentWindowMetrics().getBounds();
        } else {
            android.graphics.Point size = new android.graphics.Point();
            getWindowManager().getDefaultDisplay().getRealSize(size);
            bounds = new android.graphics.Rect(0, 0, size.x, size.y);
        }
        return bounds;
    }

    public long[] windowLayoutSnapshot() {
        synchronized (geometryLock) {
            android.graphics.Rect bounds = currentWindowBounds();
            int orientation = getResources().getConfiguration().orientation;
            SafeInsetSnapshot insets = safeInsetSnapshot;
            boolean fresh = insets.configGeneration == configGeneration
                    && insets.width == bounds.width()
                    && insets.height == bounds.height()
                    && insets.orientation == orientation;
            return new long[] {
                bounds.width(),
                bounds.height(),
                orientation,
                insets.left,
                insets.top,
                insets.right,
                insets.bottom,
                insets.width,
                insets.height,
                insets.orientation,
                configGeneration,
                insets.configGeneration,
                fresh ? 1 : 0
            };
        }
    }

    private void updateSafeInsets(android.view.View view, WindowInsets insets) {
        boolean changed;
        synchronized (geometryLock) {
            android.graphics.Rect bounds;
            WindowInsets measuredInsets;
            if (Build.VERSION.SDK_INT >= 30) {
                android.view.WindowMetrics metrics =
                        getWindowManager().getCurrentWindowMetrics();
                bounds = metrics.getBounds();
                measuredInsets = metrics.getWindowInsets();
            } else {
                bounds = currentWindowBounds();
                measuredInsets = insets;
            }
            int width = bounds.width();
            int height = bounds.height();
            if (view.getWidth() != width || view.getHeight() != height) {
                view.post(view::requestApplyInsets);
                return;
            }
            int orientation = getResources().getConfiguration().orientation;
            int left = calculateSafeInset(measuredInsets, 0, width, height, orientation);
            int top = calculateSafeInset(measuredInsets, 1, width, height, orientation);
            int right = calculateSafeInset(measuredInsets, 2, width, height, orientation);
            int bottom = calculateSafeInset(measuredInsets, 3, width, height, orientation);
            SafeInsetSnapshot previous = safeInsetSnapshot;
            changed = !previous.hasSameLayout(
                    left,
                    top,
                    right,
                    bottom,
                    width,
                    height,
                    orientation,
                    configGeneration);
            if (changed) {
                safeInsetSnapshot = new SafeInsetSnapshot(
                        left,
                        top,
                        right,
                        bottom,
                        width,
                        height,
                        orientation,
                        configGeneration);
            }
        }
        if (changed) {
            nativeRequestRepaint();
        }
    }

    private int calculateSafeInset(
            WindowInsets insets, int side, int width, int height, int orientation) {
        int safeInset = 0;
        if (Build.VERSION.SDK_INT >= 30) {
            android.graphics.Insets cutout = insets.getInsetsIgnoringVisibility(
                    WindowInsets.Type.displayCutout());
            android.graphics.Insets navigation = insets.getInsetsIgnoringVisibility(
                    WindowInsets.Type.navigationBars());
            switch (side) {
                case 0:
                    safeInset = Math.max(cutout.left, navigation.left);
                    break;
                case 1:
                    safeInset = cutout.top;
                    break;
                case 2:
                    safeInset = Math.max(cutout.right, navigation.right);
                    break;
                case 3:
                    safeInset = Math.max(cutout.bottom, navigation.bottom);
                    break;
                default:
                    return 0;
            }
        } else {
            switch (side) {
                case 0:
                    safeInset = insets.getStableInsetLeft();
                    break;
                case 1:
                    break;
                case 2:
                    safeInset = insets.getStableInsetRight();
                    break;
                case 3:
                    safeInset = insets.getStableInsetBottom();
                    break;
                default:
                    return 0;
            }
            if (Build.VERSION.SDK_INT >= 28) {
                DisplayCutout cutout = insets.getDisplayCutout();
                if (cutout != null) {
                    switch (side) {
                        case 0:
                            safeInset = Math.max(safeInset, cutout.getSafeInsetLeft());
                            break;
                        case 1:
                            safeInset = Math.max(safeInset, cutout.getSafeInsetTop());
                            break;
                        case 2:
                            safeInset = Math.max(safeInset, cutout.getSafeInsetRight());
                            break;
                        case 3:
                            safeInset = Math.max(safeInset, cutout.getSafeInsetBottom());
                            break;
                        default:
                            break;
                    }
                }
            }
        }
        return Math.max(
                safeInset,
                roundedCornerInset(insets, side, width, height, orientation));
    }

    private int roundedCornerInset(
            WindowInsets insets, int side, int width, int height, int orientation) {
        if (Build.VERSION.SDK_INT < 31) {
            return 0;
        }
        boolean landscape =
                orientation == android.content.res.Configuration.ORIENTATION_LANDSCAPE;
        RoundedCorner first;
        RoundedCorner second;
        switch (side) {
            case 0:
                if (!landscape) {
                    return 0;
                }
                first = insets.getRoundedCorner(RoundedCorner.POSITION_TOP_LEFT);
                second = insets.getRoundedCorner(RoundedCorner.POSITION_BOTTOM_LEFT);
                return Math.max(leftCornerInset(first), leftCornerInset(second));
            case 1:
                if (landscape) {
                    return 0;
                }
                first = insets.getRoundedCorner(RoundedCorner.POSITION_TOP_LEFT);
                second = insets.getRoundedCorner(RoundedCorner.POSITION_TOP_RIGHT);
                return Math.max(topCornerInset(first), topCornerInset(second));
            case 2:
                if (!landscape) {
                    return 0;
                }
                first = insets.getRoundedCorner(RoundedCorner.POSITION_TOP_RIGHT);
                second = insets.getRoundedCorner(RoundedCorner.POSITION_BOTTOM_RIGHT);
                return Math.max(
                        rightCornerInset(first, width),
                        rightCornerInset(second, width));
            case 3:
                if (landscape) {
                    return 0;
                }
                first = insets.getRoundedCorner(RoundedCorner.POSITION_BOTTOM_LEFT);
                second = insets.getRoundedCorner(RoundedCorner.POSITION_BOTTOM_RIGHT);
                return Math.max(
                        bottomCornerInset(first, height),
                        bottomCornerInset(second, height));
            default:
                return 0;
        }
    }

    private int leftCornerInset(RoundedCorner corner) {
        return corner == null ? 0 : corner.getCenter().x;
    }

    private int topCornerInset(RoundedCorner corner) {
        return corner == null ? 0 : corner.getCenter().y;
    }

    private int rightCornerInset(RoundedCorner corner, int width) {
        return corner == null ? 0 : Math.max(0, width - corner.getCenter().x);
    }

    private int bottomCornerInset(RoundedCorner corner, int height) {
        return corner == null ? 0 : Math.max(0, height - corner.getCenter().y);
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        Long operationId = pickerOperationIds.remove(requestCode);
        if (operationId == null) {
            return;
        }
        if (resultCode != RESULT_OK || data == null) {
            if (requestCode == 105 || requestCode == 106) {
                pendingDocumentContents = null;
            }
            nativeOnDocumentsSelected(requestCode, operationId, new String[0], null);
            return;
        }
        byte[] documentContents = null;
        if (requestCode == 105 || requestCode == 106) {
            documentContents = pendingDocumentContents;
            pendingDocumentContents = null;
        }
        byte[] contents = documentContents;
        DOCUMENT_EXECUTOR.execute(
                () -> processActivityResult(requestCode, operationId, data, contents));
    }

    private void processActivityResult(
            int requestCode, long operationId, Intent data, byte[] documentContents) {
        try {
            if (requestCode == 105 || requestCode == 106) {
                Uri uri = data.getData();
                if (uri == null) {
                    throw new IllegalStateException("document provider returned no output URI");
                }
                if (documentContents == null) {
                    throw new IllegalStateException("equalizer preset contents are unavailable");
                }
                try (OutputStream output = getContentResolver().openOutputStream(uri, "wt")) {
                    if (output == null) {
                        throw new IllegalStateException("could not open output document");
                    }
                    output.write(documentContents);
                }
                nativeOnDocumentsSelected(requestCode, operationId, new String[0], null);
                return;
            }
            if (requestCode == 104) {
                Uri treeUri = data.getData();
                if (treeUri == null) {
                    throw new IllegalStateException("document provider returned no directory");
                }
                getContentResolver().takePersistableUriPermission(
                        treeUri, Intent.FLAG_GRANT_READ_URI_PERMISSION);
                ArrayList<String> paths = copyTreeToPrivateStorage(treeUri);
                nativeOnDocumentsSelected(
                        requestCode,
                        operationId,
                        paths.toArray(new String[0]),
                        null);
                return;
            }
            ArrayList<Uri> uris = selectedUris(data);
            ArrayList<String> paths = new ArrayList<>(uris.size());
            for (Uri uri : uris) {
                try {
                    getContentResolver().takePersistableUriPermission(
                            uri, Intent.FLAG_GRANT_READ_URI_PERMISSION);
                } catch (SecurityException ignored) {
                    // Some providers grant only a temporary read permission.
                }
                paths.add(copyToPrivateStorage(uri).getAbsolutePath());
            }
            nativeOnDocumentsSelected(
                    requestCode, operationId, paths.toArray(new String[0]), null);
        } catch (Exception error) {
            nativeOnDocumentsSelected(
                    requestCode,
                    operationId,
                    new String[0],
                    "Failed to process selected document: " + error);
        }
    }

    private ArrayList<Uri> selectedUris(Intent data) {
        ArrayList<Uri> uris = new ArrayList<>();
        ClipData clipData = data.getClipData();
        if (clipData != null) {
            for (int index = 0; index < clipData.getItemCount(); index++) {
                uris.add(clipData.getItemAt(index).getUri());
            }
        } else if (data.getData() != null) {
            uris.add(data.getData());
        }
        return uris;
    }

    private File copyToPrivateStorage(Uri uri) throws Exception {
        File importDir = new File(getFilesDir(), "imports");
        if (!importDir.isDirectory() && !importDir.mkdirs()) {
            throw new IllegalStateException("cannot create " + importDir);
        }
        String displayName = sanitizeFileName(queryDisplayName(uri));
        File output = uniqueFile(importDir, displayName);
        try (InputStream input = getContentResolver().openInputStream(uri);
                FileOutputStream stream = new FileOutputStream(output)) {
            if (input == null) {
                throw new IllegalStateException("document provider returned no input stream");
            }
            byte[] buffer = new byte[64 * 1024];
            int count;
            while ((count = input.read(buffer)) >= 0) {
                stream.write(buffer, 0, count);
            }
        }
        return output;
    }

    private ArrayList<String> copyTreeToPrivateStorage(Uri treeUri) throws Exception {
        File importDir = new File(getFilesDir(), "imports");
        if (!importDir.isDirectory() && !importDir.mkdirs()) {
            throw new IllegalStateException("cannot create " + importDir);
        }
        String rootId = DocumentsContract.getTreeDocumentId(treeUri);
        Uri rootUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, rootId);
        String rootName = queryDocumentName(rootUri);
        File output = uniqueDirectory(importDir, sanitizeFileName(rootName));
        if (!output.mkdirs()) {
            throw new IllegalStateException("cannot create " + output);
        }
        ArrayList<String> copiedPaths = new ArrayList<>();
        copyDocumentChildren(treeUri, rootId, output, copiedPaths);
        return copiedPaths;
    }

    private void copyDocumentChildren(
            Uri treeUri,
            String parentId,
            File outputDir,
            ArrayList<String> copiedPaths)
            throws Exception {
        Uri children = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, parentId);
        String[] columns = {
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE
        };
        try (Cursor cursor = getContentResolver().query(children, columns, null, null, null)) {
            if (cursor == null) {
                throw new IllegalStateException("document provider returned no directory listing");
            }
            int idColumn = cursor.getColumnIndexOrThrow(columns[0]);
            int nameColumn = cursor.getColumnIndexOrThrow(columns[1]);
            int mimeColumn = cursor.getColumnIndexOrThrow(columns[2]);
            while (cursor.moveToNext()) {
                String documentId = cursor.getString(idColumn);
                String name = sanitizeFileName(cursor.getString(nameColumn));
                String mimeType = cursor.getString(mimeColumn);
                Uri documentUri =
                        DocumentsContract.buildDocumentUriUsingTree(treeUri, documentId);
                if (DocumentsContract.Document.MIME_TYPE_DIR.equals(mimeType)) {
                    File childDir = uniqueDirectory(outputDir, name);
                    if (!childDir.mkdirs()) {
                        throw new IllegalStateException("cannot create " + childDir);
                    }
                    copyDocumentChildren(treeUri, documentId, childDir, copiedPaths);
                } else {
                    File output = uniqueFile(outputDir, name);
                    copyDocumentToFile(documentUri, output);
                    copiedPaths.add(output.getAbsolutePath());
                }
            }
        }
    }

    private void copyDocumentToFile(Uri uri, File output) throws Exception {
        try (InputStream input = getContentResolver().openInputStream(uri);
                FileOutputStream stream = new FileOutputStream(output)) {
            if (input == null) {
                throw new IllegalStateException("document provider returned no input stream");
            }
            byte[] buffer = new byte[64 * 1024];
            int count;
            while ((count = input.read(buffer)) >= 0) {
                stream.write(buffer, 0, count);
            }
        }
    }

    private String queryDocumentName(Uri uri) {
        try (Cursor cursor = getContentResolver().query(
                uri,
                new String[] {DocumentsContract.Document.COLUMN_DISPLAY_NAME},
                null,
                null,
                null)) {
            if (cursor != null && cursor.moveToFirst()) {
                return cursor.getString(0);
            }
        }
        return "directory";
    }

    private String queryDisplayName(Uri uri) {
        try (Cursor cursor = getContentResolver().query(
                uri, new String[] {OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (cursor != null && cursor.moveToFirst()) {
                int column = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME);
                if (column >= 0) {
                    String name = cursor.getString(column);
                    if (name != null && !name.isEmpty()) {
                        return name;
                    }
                }
            }
        }
        String segment = uri.getLastPathSegment();
        return segment == null ? "document" : segment;
    }

    private String sanitizeFileName(String name) {
        String sanitized = name.replaceAll("[/\\\\\\n\\r\\t]", "_");
        return sanitized.isEmpty() ? "document" : sanitized;
    }

    private File uniqueFile(File directory, String name) {
        File candidate = new File(directory, name);
        if (!candidate.exists()) {
            return candidate;
        }
        int dot = name.lastIndexOf('.');
        String stem = dot > 0 ? name.substring(0, dot) : name;
        String extension = dot > 0 ? name.substring(dot) : "";
        for (int suffix = 2; ; suffix++) {
            candidate = new File(directory, stem + "-" + suffix + extension);
            if (!candidate.exists()) {
                return candidate;
            }
        }
    }

    private File uniqueDirectory(File directory, String name) {
        File candidate = new File(directory, name);
        if (!candidate.exists()) {
            return candidate;
        }
        for (int suffix = 2; ; suffix++) {
            candidate = new File(directory, name + "-" + suffix);
            if (!candidate.exists()) {
                return candidate;
            }
        }
    }
}
