package org.xmms.renascene;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.pm.ApplicationInfo;
import android.content.pm.ServiceInfo;
import android.graphics.Bitmap;
import android.media.AudioAttributes;
import android.media.AudioFocusRequest;
import android.media.AudioManager;
import android.media.MediaDescription;
import android.media.MediaMetadata;
import android.media.browse.MediaBrowser;
import android.media.session.MediaSession;
import android.media.session.PlaybackState;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.os.PowerManager;
import android.os.SystemClock;
import android.service.media.MediaBrowserService;
import android.util.Log;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Locale;

/**
 * Main-thread Android owner for MediaSession, foreground playback, wake lock, and audio focus.
 *
 * <p>The generated manifest does not assign a separate {@code android:process}, but this service
 * can exist with no Activity. Rust transport callbacks execute the process-wide shared backend
 * synchronously, then enqueue the matching domain transition tagged as already executed. This is
 * required because an existing but paused Activity cannot drain egui events from a home-screen
 * widget. MediaBrowser query JNI calls are synchronous read-only endpoints because Android
 * requires immediate results.
 */
public final class XmmsPlaybackService extends MediaBrowserService {
    static final String ACTION_UPDATE = "org.xmms.renascene.service.UPDATE";
    static final String ACTION_ARTWORK_INVALIDATE = "org.xmms.renascene.service.ARTWORK_INVALIDATE";
    static final String EXTRA_PROJECTION_TOKEN = "projectionToken";
    static final String ACTION_WIDGET_CONTROL =
            "org.xmms.renascene.service.WIDGET_CONTROL";
    static final String EXTRA_WIDGET_CONTROL = "widgetControl";
    static final String EXTRA_STATE = "state";
    static final String EXTRA_TITLE = "title";
    static final String EXTRA_BITRATE = "bitrate";
    static final String EXTRA_FREQUENCY = "frequency";
    static final String EXTRA_CHANNELS = "channels";
    static final String EXTRA_DURATION_MS = "durationMs";
    static final String EXTRA_POSITION_MS = "positionMs";
    static final String EXTRA_CURRENT_INDEX = "currentIndex";
    static final String EXTRA_PLAYLIST_SIZE = "playlistSize";
    static final String EXTRA_HAS_PREVIOUS = "hasPrevious";
    static final String EXTRA_HAS_NEXT = "hasNext";

    private static final String ROOT_ID = "xmms-root";
    private static final String PLAYLIST_ID = "xmms-playlist";
    private static final String MEDIA_ID_PREFIX = "track:";
    private static final String TAG = "XmmsPlaybackService";
    private static final String CHANNEL_ID = "xmms_playback";
    private static final int NOTIFICATION_ID = 1;
    static final int CONTROL_PAUSE = 1;
    static final int CONTROL_PLAY = 2;
    static final int CONTROL_NEXT = 3;
    static final int CONTROL_PREVIOUS = 4;
    private static final int CONTROL_SEEK = 5;
    static final int CONTROL_STOP = 6;
    private static final int CONTROL_PLAY_MEDIA_ITEM = 7;
    private static final long PLAYING_POLL_INTERVAL_MS = 500;
    private static final long PAUSED_POLL_INTERVAL_MS = 1000;

    /** Playback state values received from the Rust JNI layer via applyNativePlaybackState(). */
    private static final int STATE_STOPPED = 0;
    private static final int STATE_PLAYING = 1;
    @SuppressWarnings("unused") // retained for documentation; received from JNI
    private static final int STATE_PAUSED = 2;

    static {
        System.loadLibrary("xmms_renascene");
    }

    private native void nativeOnMediaControl(int control, long value);
    private native void nativePollPlayback();
    private native void nativeInitializeMediaLibrary(String filesDir, String cacheDir);
    private native int nativeMediaItemCount();
    private native String nativeMediaItemTitle(int index);
    private native long nativeMediaItemDurationMs(int index);
    private native long nativeCurrentMediaItemIndex();
    private native String nativeMediaQueueSnapshot();
    private native boolean nativeValidateProjection(String token, boolean requireOwner);
    private native void nativeRefreshPlayback();
    private native String nativeCurrentProjectionToken();
    private native boolean nativeDebugReversePlaylist();
    private native boolean nativeDebugReplaceCurrent(boolean clear);
    private native boolean nativeDebugPalette(int serial);

    private PowerManager.WakeLock playbackWakeLock;
    private NotificationManager notificationManager;
    private AudioManager audioManager;
    private AudioFocusRequest audioFocusRequest;
    private MediaSession mediaSession;
    private XmmsArtworkWorker artworkWorker;
    private XmmsBrowseArtwork browseArtwork;
    private String artworkFilesDir;
    private boolean artworkDebuggable;
    private Bitmap readyArtwork;
    private Uri readyArtworkUri;
    private boolean highResolutionArtwork;
    private XmmsArtworkWorker.Snapshot readyArtworkSnapshot;
    private String acceptedProjection = "";
    private List<Object> lastMetadata;
    private List<Object> lastNotification;
    private boolean foregroundActive;
    private boolean debugMutationPending;
    private long metadataPublications;
    private long notificationPublications;
    private long rejectedProjections;
    private long metadataPublishedNs;
    private long metadataCallNs;
    private long notificationCallNs;
    private Intent debugHeldProjection;
    private long debugHeldGeneration;
    private boolean audioFocusHeld;
    private volatile boolean resumeAfterFocusGain;
    private boolean noisyReceiverRegistered;
    private final BroadcastReceiver noisyReceiver = new BroadcastReceiver() {
        @Override
        public void onReceive(Context context, Intent intent) {
            if ("org.xmms.renascene.service.ARTWORK_RECOVER".equals(intent.getAction())) {
                if (playbackState != STATE_STOPPED && artworkWorker != null) artworkWorker.recoverUri();
                return;
            }
            if (ACTION_ARTWORK_INVALIDATE.equals(intent.getAction())) {
                if (playbackState != STATE_STOPPED) {
                    refreshArtworkCurrent(true);
                    publishArtworkOnly();
                }
                return;
            }
            if (AudioManager.ACTION_AUDIO_BECOMING_NOISY.equals(intent.getAction())
                    && playbackState == STATE_PLAYING) {
                resumeAfterFocusGain = false;
                nativeOnMediaControl(CONTROL_PAUSE, 0);
            }
        }
    };
    private final Handler playbackHandler = new Handler(Looper.getMainLooper());
    private final Runnable playbackPoll = new Runnable() {
        @Override
        public void run() {
            if (playbackState != STATE_STOPPED) {
                nativePollPlayback();
            }
            playbackHandler.postDelayed(
                    this,
                    playbackState == STATE_PLAYING
                            ? PLAYING_POLL_INTERVAL_MS
                            : PAUSED_POLL_INTERVAL_MS);
        }
    };

    private final Runnable refreshProjection = () -> {
        nativeRefreshPlayback();
        if (this.playbackState == STATE_STOPPED) stopPlaybackService();
    };

    private int playbackState;
    private String playbackTitle = "XMMS Renascene";
    private int playbackBitrate;
    private int playbackFrequency;
    private int playbackChannels;
    private long playbackDurationMs = -1;
    private long playbackPositionMs;
    private long currentMediaItemIndex = -1;
    private int playlistSize;
    private long mediaQueueVersion = Long.MIN_VALUE;
    private boolean hasPrevious;
    private boolean hasNext;

    @Override
    public void onCreate() {
        super.onCreate();
        artworkFilesDir = getFilesDir().getAbsolutePath();
        artworkDebuggable = (getApplicationInfo().flags & ApplicationInfo.FLAG_DEBUGGABLE) != 0;
        nativeInitializeMediaLibrary(artworkFilesDir, getCacheDir().getAbsolutePath());
        notificationManager =
                (NotificationManager) getSystemService(NOTIFICATION_SERVICE);
        NotificationChannel channel = new NotificationChannel(
                CHANNEL_ID, "Playback", NotificationManager.IMPORTANCE_LOW);
        channel.setDescription("XMMS Renascene playback controls");
        channel.setShowBadge(false);
        notificationManager.createNotificationChannel(channel);

        PowerManager powerManager = (PowerManager) getSystemService(POWER_SERVICE);
        playbackWakeLock = powerManager.newWakeLock(
                PowerManager.PARTIAL_WAKE_LOCK, "xmms-renascene:playback");
        playbackWakeLock.setReferenceCounted(false);

        audioManager = (AudioManager) getSystemService(AUDIO_SERVICE);
        AudioAttributes attributes = new AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_MEDIA)
                .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                .build();
        audioFocusRequest = new AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
                .setAudioAttributes(attributes)
                .setOnAudioFocusChangeListener(this::handleAudioFocusChange)
                .build();
        IntentFilter noisyFilter = new IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY);
        noisyFilter.addAction(ACTION_ARTWORK_INVALIDATE);
        noisyFilter.addAction("org.xmms.renascene.service.ARTWORK_RECOVER");
        if (Build.VERSION.SDK_INT >= 33) {
            registerReceiver(noisyReceiver, noisyFilter, RECEIVER_NOT_EXPORTED);
        } else {
            registerReceiver(noisyReceiver, noisyFilter);
        }
        noisyReceiverRegistered = true;

        mediaSession = new MediaSession(this, "XMMS Renascene");
        mediaSession.setFlags(
                MediaSession.FLAG_HANDLES_MEDIA_BUTTONS
                        | MediaSession.FLAG_HANDLES_TRANSPORT_CONTROLS);
        mediaSession.setSessionActivity(activityPendingIntent());
        mediaSession.setCallback(new MediaSession.Callback() {
            @Override
            public void onPlay() {
                dispatchMediaControl(CONTROL_PLAY, 0);
            }

            @Override
            public void onPause() {
                dispatchMediaControl(CONTROL_PAUSE, 0);
            }

            @Override
            public void onSkipToNext() {
                dispatchMediaControl(CONTROL_NEXT, 0);
            }

            @Override
            public void onSkipToPrevious() {
                dispatchMediaControl(CONTROL_PREVIOUS, 0);
            }

            @Override
            public void onSeekTo(long positionMs) {
                dispatchMediaControl(CONTROL_SEEK, positionMs);
            }

            @Override
            public void onStop() {
                dispatchMediaControl(CONTROL_STOP, 0);
            }

            @Override
            public void onPlayFromMediaId(String mediaId, Bundle extras) {
                playMediaId(mediaId);
            }

            @Override
            public void onSkipToQueueItem(long id) {
                playMediaItem(id);
            }

            @Override
            public void onPlayFromSearch(String query, Bundle extras) {
                playMediaItem(findMediaItem(query));
            }

            @Override
            public void onCustomAction(String action, Bundle extras) {
                if (artworkDebuggable && "xmms-debug-artwork-worker".equals(action)) {
                    debugArtworkWorker(extras == null ? Bundle.EMPTY : extras);
                }
            }
        });
        setSessionToken(mediaSession.getSessionToken());
        ensureArtworkWorker();
        refreshMediaQueue();
        playbackHandler.post(playbackPoll);
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        if (intent != null && ACTION_WIDGET_CONTROL.equals(intent.getAction())) {
            if (Build.VERSION.SDK_INT >= 26) {
                startForeground(NOTIFICATION_ID, buildNotification(false));
                foregroundActive = true;
            }
            int control = intent.getIntExtra(EXTRA_WIDGET_CONTROL, CONTROL_PLAY);
            if (!dispatchMediaControl(control, 0)) {
                stopPlaybackService();
                return START_NOT_STICKY;
            }
            playbackHandler.postDelayed(() -> {
                if (playbackState == STATE_STOPPED) {
                    stopPlaybackService();
                }
            }, 1500);
            return START_NOT_STICKY;
        }
        if (intent == null || !ACTION_UPDATE.equals(intent.getAction())) {
            stopPlaybackService();
            return START_NOT_STICKY;
        }
        if (artworkDebuggable) debugHeldProjection = new Intent(intent);
        applyNativePlaybackState(
                intent.getIntExtra(EXTRA_STATE, 0),
                intent.getStringExtra(EXTRA_TITLE),
                intent.getIntExtra(EXTRA_BITRATE, 0),
                intent.getIntExtra(EXTRA_FREQUENCY, 0),
                intent.getIntExtra(EXTRA_CHANNELS, 0),
                intent.getLongExtra(EXTRA_DURATION_MS, -1),
                intent.getLongExtra(EXTRA_POSITION_MS, 0),
                intent.getLongExtra(EXTRA_CURRENT_INDEX, -1),
                intent.getIntExtra(EXTRA_PLAYLIST_SIZE, 0),
                intent.getBooleanExtra(EXTRA_HAS_PREVIOUS, false),
                intent.getBooleanExtra(EXTRA_HAS_NEXT, false),
                intent.getStringExtra(EXTRA_PROJECTION_TOKEN));
        return START_NOT_STICKY;
    }

    public void applyNativePlaybackState(
            int state,
            String title,
            int bitrate,
            int frequency,
            int channels,
            long durationMs,
            long positionMs,
            long currentIndex,
            int mediaItemCount,
            boolean previous,
            boolean next,
            String projectionToken) {
        if (!nativeValidateProjection(projectionToken, true)) {
            rejectedProjections++;
            // A stale foreground packet must not restore an old title/index.
            // Refresh synchronously from current native state instead; this also
            // satisfies the initial foreground-service deadline when necessary.
            playbackHandler.removeCallbacks(refreshProjection);
            playbackHandler.post(refreshProjection);
            return;
        }
        acceptedProjection = projectionToken;
        String normalizedTitle =
                title == null || title.isEmpty() ? "XMMS Renascene" : title;
        int normalizedBitrate = Math.max(0, bitrate);
        int normalizedFrequency = Math.max(0, frequency);
        int normalizedChannels = Math.max(0, channels);
        boolean playbackChanged = playbackState != state;
        boolean infoChanged =
                !playbackTitle.equals(normalizedTitle)
                        || playbackBitrate != normalizedBitrate
                        || playbackFrequency != normalizedFrequency
                        || playbackChannels != normalizedChannels;
        playbackState = state;
        playbackTitle = normalizedTitle;
        playbackBitrate = normalizedBitrate;
        playbackFrequency = normalizedFrequency;
        playbackChannels = normalizedChannels;
        playbackDurationMs = durationMs;
        playbackPositionMs = Math.max(0, positionMs);
        currentMediaItemIndex = currentIndex;
        playlistSize = Math.max(0, mediaItemCount);
        hasPrevious = previous;
        hasNext = next;
        boolean playing = state == STATE_PLAYING;
        if (playing && !requestPlaybackAudioFocus()) {
            Log.w(TAG, "Pausing playback because audio focus was denied");
            playbackState = STATE_PAUSED;
            nativeOnMediaControl(CONTROL_PAUSE, 0);
            stopPlaybackService();
            return;
        }
        XmmsPlayerWidget.updateAll(
                this,
                hasPrevious,
                hasNext);
        if (state != STATE_STOPPED && (infoChanged || playbackChanged)) {
            XmmsPlayerInfoWidget.updateAll(
                    this,
                    playbackState,
                    playbackTitle,
                    playbackBitrate,
                    playbackFrequency,
                    playbackChannels);
        }
        refreshMediaQueue();

        if (state == STATE_STOPPED) {
            stopPlaybackService();
            return;
        }
        updateWakeLock(playing);
        refreshArtworkCurrent(true);
        updateMediaSession(playing);
        publishNotification(playing);
    }

    public void applyNativePlaybackPosition(long positionMs) {
        playbackPositionMs = Math.max(0, positionMs);
        updatePlaybackState(playbackState == STATE_PLAYING);
    }

    @Override
    public void onDestroy() {
        closeArtworkWorker();
        if (browseArtwork != null) { browseArtwork.close(); browseArtwork = null; }
        playbackHandler.removeCallbacks(playbackPoll);
        playbackHandler.removeCallbacks(refreshProjection);
        if (noisyReceiverRegistered) {
            unregisterReceiver(noisyReceiver);
            noisyReceiverRegistered = false;
        }
        releaseWakeLock();
        abandonAudioFocus();
        if (mediaSession != null) {
            mediaSession.release();
            mediaSession = null;
        }
        if (Build.VERSION.SDK_INT >= 24) {
            stopForeground(STOP_FOREGROUND_REMOVE);
        } else {
            stopForeground(true);
        }
        super.onDestroy();
    }

    @Override
    public BrowserRoot onGetRoot(
            String clientPackageName,
            int clientUid,
            Bundle rootHints) {
        if ((rootHints != null && rootHints.getBoolean("xmms.artwork.high_resolution", false))
                || "com.android.car.media".equals(clientPackageName)
                || "com.google.android.projection.gearhead".equals(clientPackageName)) {
            highResolutionArtwork = true;
            ensureArtworkWorker(); artworkWorker.enableHighResolution();
            if (browseArtwork == null) browseArtwork = new XmmsBrowseArtwork(getApplicationContext(), artworkFilesDir,
                    playbackHandler, () -> notifyChildrenChanged(PLAYLIST_ID));
        }
        return new BrowserRoot(ROOT_ID, null);
    }

    @Override
    public void onLoadChildren(
            String parentId,
            Result<List<MediaBrowser.MediaItem>> result) {
        if (ROOT_ID.equals(parentId)) {
            MediaDescription playlist = new MediaDescription.Builder()
                    .setMediaId(PLAYLIST_ID)
                    .setTitle("Current playlist")
                    .setSubtitle(playlistSize + (playlistSize == 1 ? " track" : " tracks"))
                    .setIconUri(iconUri())
                    .build();
            result.sendResult(Collections.singletonList(
                    new MediaBrowser.MediaItem(
                            playlist,
                            MediaBrowser.MediaItem.FLAG_BROWSABLE)));
            return;
        }
        if (PLAYLIST_ID.equals(parentId)) {
            result.sendResult(mediaItems(0, nativeMediaItemCount()));
            return;
        }
        result.sendResult(Collections.emptyList());
    }

    @Override
    public void onLoadChildren(
            String parentId,
            Result<List<MediaBrowser.MediaItem>> result,
            Bundle options) {
        if (!PLAYLIST_ID.equals(parentId)) {
            onLoadChildren(parentId, result);
            return;
        }
        int count = nativeMediaItemCount();
        int page = options.getInt(MediaBrowser.EXTRA_PAGE, -1);
        int pageSize = options.getInt(MediaBrowser.EXTRA_PAGE_SIZE, -1);
        if (page < 0 || pageSize <= 0) {
            result.sendResult(mediaItems(0, count));
            return;
        }
        long requestedStart = (long) page * pageSize;
        if (requestedStart >= count) {
            result.sendResult(Collections.emptyList());
            return;
        }
        int start = (int) requestedStart;
        result.sendResult(mediaItems(start, (int) Math.min((long) count, (long) start + pageSize), pageSize <= 32));
    }

    private void updateMediaSession(boolean playing) {
        publishMetadata();
        updatePlaybackState(playing);
        mediaSession.setActive(true);
    }

    private void publishMetadata() {
        String track = readyArtworkSnapshot == null ? "" : readyArtworkSnapshot.track;
        String key = readyArtwork == null || readyArtworkSnapshot == null ? "" : readyArtworkSnapshot.key;
        List<Object> fields = Arrays.asList(playbackTitle, playbackDurationMs, currentMediaItemIndex, track, key, readyArtworkUri);
        if (fields.equals(lastMetadata)) return;
        MediaMetadata.Builder metadata = new MediaMetadata.Builder()
                .putString(MediaMetadata.METADATA_KEY_TITLE, playbackTitle)
                .putString(MediaMetadata.METADATA_KEY_DISPLAY_TITLE, playbackTitle)
                .putString(MediaMetadata.METADATA_KEY_MEDIA_ID, mediaId(currentMediaItemIndex));
        if (playbackDurationMs >= 0) {
            metadata.putLong(MediaMetadata.METADATA_KEY_DURATION, playbackDurationMs);
        }
        if (readyArtwork != null) metadata.putBitmap(MediaMetadata.METADATA_KEY_ALBUM_ART, readyArtwork);
        if (readyArtworkUri != null) {
            metadata.putString(MediaMetadata.METADATA_KEY_ALBUM_ART_URI, readyArtworkUri.toString())
                .putString(MediaMetadata.METADATA_KEY_ART_URI, readyArtworkUri.toString())
                .putString(MediaMetadata.METADATA_KEY_DISPLAY_ICON_URI, readyArtworkUri.toString());
        }
        XmmsArtworkCache.pin(readyArtworkUri);
        long startedNs = SystemClock.elapsedRealtimeNanos();
        mediaSession.setMetadata(metadata.build());
        metadataPublishedNs = SystemClock.elapsedRealtimeNanos();
        metadataCallNs = metadataPublishedNs - startedNs;
        lastMetadata = fields;
        metadataPublications++;
    }

    private void updatePlaybackState(boolean playing) {
        long actions = PlaybackState.ACTION_PLAY
                | PlaybackState.ACTION_PAUSE
                | PlaybackState.ACTION_PLAY_PAUSE
                | PlaybackState.ACTION_SEEK_TO
                | PlaybackState.ACTION_STOP;
        if (hasPrevious) {
            actions |= PlaybackState.ACTION_SKIP_TO_PREVIOUS;
        }
        if (hasNext) {
            actions |= PlaybackState.ACTION_SKIP_TO_NEXT;
        }
        mediaSession.setPlaybackState(new PlaybackState.Builder()
                .setActions(actions)
                .setState(
                        playing ? PlaybackState.STATE_PLAYING : PlaybackState.STATE_PAUSED,
                        playbackPositionMs,
                        playing ? 1.0f : 0.0f)
                .setActiveQueueItemId(currentMediaItemIndex)
                .build());
    }

    private void refreshMediaQueue() {
        if (mediaSession == null) {
            return;
        }
        String snapshot = nativeMediaQueueSnapshot();
        if (snapshot == null || snapshot.isEmpty()) {
            return;
        }
        try {
            JSONObject root = new JSONObject(snapshot);
            long version = root.optLong("version", 0);
            if (version == mediaQueueVersion) {
                return;
            }
            JSONArray items = root.optJSONArray("items");
            int count = items == null ? 0 : items.length();
            List<MediaSession.QueueItem> queue = new ArrayList<>(count);
            for (int index = 0; index < count; index++) {
                JSONObject item = items.optJSONObject(index);
                String title = item == null ? "" : item.optString("title", "");
                long durationMs = item == null ? -1 : item.optLong("durationMs", -1);
                queue.add(new MediaSession.QueueItem(
                        mediaDescription(index, title, durationMs), index));
            }
            playlistSize = count;
            mediaSession.setQueue(queue);
            mediaSession.setQueueTitle("Current playlist");
            mediaQueueVersion = version;
            notifyChildrenChanged(PLAYLIST_ID);
        } catch (JSONException error) {
            Log.e(TAG, "Failed to parse native media queue snapshot", error);
        }
        long nativeIndex = nativeCurrentMediaItemIndex();
        if (nativeIndex >= 0) {
            currentMediaItemIndex = nativeIndex;
        }
    }

    private List<MediaBrowser.MediaItem> mediaItems(int start, int end) {
        return mediaItems(start, end, false);
    }

    private List<MediaBrowser.MediaItem> mediaItems(int start, int end, boolean requestArtwork) {
        List<MediaBrowser.MediaItem> items = new ArrayList<>(Math.max(0, end - start));
        for (int index = start; index < end; index++) {
            Uri art = browseArtwork == null ? null : browseArtwork.icon(index, requestArtwork);
            items.add(new MediaBrowser.MediaItem(
                    mediaDescription(index, nativeMediaItemTitle(index), nativeMediaItemDurationMs(index), art),
                    MediaBrowser.MediaItem.FLAG_PLAYABLE));
        }
        return items;
    }

    private MediaDescription mediaDescription(int index) {
        String title = nativeMediaItemTitle(index);
        long durationMs = nativeMediaItemDurationMs(index);
        return mediaDescription(index, title, durationMs);
    }

    private MediaDescription mediaDescription(int index, String title, long durationMs) {
        return mediaDescription(index, title, durationMs, null);
    }

    private MediaDescription mediaDescription(int index, String title, long durationMs, Uri artworkUri) {
        if (title == null || title.isEmpty()) {
            title = "Track " + (index + 1);
        }
        Bundle extras = new Bundle();
        if (durationMs >= 0) {
            extras.putLong(MediaMetadata.METADATA_KEY_DURATION, durationMs);
        }
        return new MediaDescription.Builder()
                .setMediaId(mediaId(index))
                .setTitle(title)
                .setSubtitle("XMMS Renascene")
                .setIconUri(artworkUri == null ? iconUri() : artworkUri)
                .setExtras(extras)
                .build();
    }

    private void playMediaId(String mediaId) {
        if (mediaId == null || !mediaId.startsWith(MEDIA_ID_PREFIX)) {
            return;
        }
        try {
            playMediaItem(Long.parseLong(mediaId.substring(MEDIA_ID_PREFIX.length())));
        } catch (NumberFormatException error) {
            Log.w(TAG, "Ignoring malformed media ID: " + mediaId, error);
        }
    }

    private void playMediaItem(long index) {
        if (index < 0 || index >= nativeMediaItemCount()) {
            return;
        }
        dispatchMediaControl(CONTROL_PLAY_MEDIA_ITEM, index);
    }

    private long findMediaItem(String query) {
        int count = nativeMediaItemCount();
        if (count == 0) {
            return -1;
        }
        if (query == null || query.trim().isEmpty()) {
            return currentMediaItemIndex >= 0 ? currentMediaItemIndex : 0;
        }
        String normalized = query.toLowerCase(Locale.ROOT);
        for (int index = 0; index < count; index++) {
            String title = nativeMediaItemTitle(index);
            if (title != null && title.toLowerCase(Locale.ROOT).contains(normalized)) {
                return index;
            }
        }
        return 0;
    }

    private void ensureArtworkWorker() {
        if (artworkWorker != null && !artworkWorker.isClosed()) return;
        artworkWorker = new XmmsArtworkWorker(getApplicationContext(), artworkFilesDir, playbackHandler, artworkDebuggable,
                new XmmsArtworkWorker.Listener() {
                    @Override public void onReady(XmmsArtworkWorker.Snapshot snapshot, Bitmap bitmap) {
                        if (playbackState != STATE_STOPPED && nativeValidateProjection(acceptedProjection, false)) {
                            XmmsArtworkWorker.Snapshot current = artworkWorker.currentSnapshot();
                            if (snapshot != null && current != null && !snapshot.raw.equals(current.raw)) return;
                            readyArtworkSnapshot = current;
                            readyArtwork = bitmap;
                            readyArtworkUri = bitmap == null ? null : artworkWorker.currentUri();
                            publishArtworkOnly();
                        }
                    }
                    @Override public void onDiagnostic(String status) {
                        if (artworkDebuggable) sendArtworkWorkerEvent(artworkWorker);
                    }
                });
    }

    private void refreshArtworkCurrent(boolean request) {
        ensureArtworkWorker();
        XmmsArtworkWorker.Snapshot current = artworkWorker.currentSnapshot();
        if (request) artworkWorker.noteRequest(current);
        if (current == null || !current.resolved || !current.enabled || !current.hasTrack) {
            readyArtwork = null;
        } else if (readyArtworkSnapshot == null || !readyArtworkSnapshot.key.equals(current.key)) {
            readyArtwork = artworkWorker.cachedBitmap(current);
        }
        readyArtworkSnapshot = current;
        readyArtworkUri = readyArtwork == null ? null : artworkWorker.cachedUri(current);
        if (highResolutionArtwork) artworkWorker.enableHighResolution();
        if (request) artworkWorker.requestLater();
    }

    private void publishArtworkOnly() {
        if (playbackState == STATE_STOPPED || !foregroundActive || mediaSession == null) return;
        if (!nativeValidateProjection(acceptedProjection, false)) return;
        publishMetadata();
        publishNotification(playbackState == STATE_PLAYING);
    }

    private void publishNotification(boolean playing) {
        String key = readyArtwork == null || readyArtworkSnapshot == null ? "" : readyArtworkSnapshot.key;
        List<Object> fields = Arrays.asList(playbackTitle, playing, hasPrevious, hasNext, key);
        if (foregroundActive && fields.equals(lastNotification)) return;
        long startedNs = SystemClock.elapsedRealtimeNanos();
        Notification notification = buildNotification(playing);
        if (!foregroundActive) {
            if (Build.VERSION.SDK_INT >= 29) startForeground(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK);
            else startForeground(NOTIFICATION_ID, notification);
            foregroundActive = true;
        } else notificationManager.notify(NOTIFICATION_ID, notification);
        lastNotification = fields;
        notificationPublications++;
        notificationCallNs = SystemClock.elapsedRealtimeNanos() - startedNs;
    }

    private void closeArtworkWorker() {
        if (artworkWorker != null) artworkWorker.close();
        readyArtwork = null; readyArtworkSnapshot = null; readyArtworkUri = null;
        XmmsArtworkCache.pin(null);
    }

    private void sendArtworkWorkerEvent(XmmsArtworkWorker worker) {
        if (mediaSession == null || worker == null || !artworkDebuggable) return;
        Bundle event = new Bundle();
        JSONObject stats = worker.stats();
        try {
            if (browseArtwork != null) stats.put("browseArtwork", browseArtwork.metrics());
            stats.put("metadataPublications", metadataPublications).put("notificationPublications", notificationPublications)
                .put("rejectedProjections", rejectedProjections).put("hasMetadataArt", readyArtwork != null && playbackState != STATE_STOPPED)
                .put("metadataPublishedNs", metadataPublishedNs).put("metadataCallNs", metadataCallNs)
                .put("notificationCallNs", notificationCallNs).put("eventNs", SystemClock.elapsedRealtimeNanos())
                .put("mutationPending", debugMutationPending);
        } catch (JSONException impossible) { throw new IllegalStateException(impossible); }
        event.putString("stats", stats.toString());
        mediaSession.sendSessionEvent("xmms-artwork-worker", event);
    }

    public boolean isArtworkDebuggable() { return artworkDebuggable; }

    private void debugArtworkWorker(Bundle extras) {
        // This reachable test endpoint is unavailable in non-debuggable APKs.
        if (!artworkDebuggable) return;
        String command = extras.getString("command", "run");
        if ("publish".equals(command)) {
            // Use the real started-service/widget path: a browser-bound-only
            // service otherwise disappears when the debug client disconnects.
            startForegroundService(new Intent(this, XmmsPlaybackService.class)
                    .setAction(ACTION_WIDGET_CONTROL).putExtra(EXTRA_WIDGET_CONTROL, CONTROL_PLAY));
            return;
        }
        if ("close".equals(command)) {
            closeArtworkWorker(); sendArtworkWorkerEvent(artworkWorker); return;
        }
        if ("stats".equals(command)) { sendArtworkWorkerEvent(artworkWorker); return; }
        if ("replace".equals(command)) {
            closeArtworkWorker();
            ensureArtworkWorker();
            if (playbackState != STATE_STOPPED) { refreshArtworkCurrent(true); publishArtworkOnly(); return; }
        }
        ensureArtworkWorker();
        artworkWorker.debugConfigure(extras.getBoolean("trace", false), extras.getInt("cold_delay_ms", 0));
        if ("replay".equals(command)) {
            if (debugHeldProjection != null) {
                Intent delayed = new Intent(debugHeldProjection);
                long ticket = ++debugHeldGeneration;
                playbackHandler.postDelayed(() -> {
                    if (ticket == debugHeldGeneration && playbackState != STATE_STOPPED) onStartCommand(delayed, 0, 0);
                }, Math.max(0, Math.min(2000, extras.getInt("projection_delay_ms", 500))));
            }
            return;
        }
        if ("replace-current".equals(command) || "clear-playlist".equals(command)) {
            debugMutationPending = true;
            nativeDebugReplaceCurrent("clear-playlist".equals(command));
            playbackHandler.postDelayed(() -> { debugMutationPending = false; sendArtworkWorkerEvent(artworkWorker); }, 300);
            return;
        }
        if ("perf-palette".equals(command)) {
            nativeDebugPalette(extras.getInt("palette_serial", 0));
            refreshArtworkCurrent(true); publishArtworkOnly(); return;
        }
        if ("timed-skip".equals(command)) {
            int count = Math.max(1, Math.min(100, extras.getInt("count", 100)));
            int interval = Math.max(10, Math.min(1000, extras.getInt("interval_ms", 100)));
            long ticket = ++debugHeldGeneration;
            debugMutationPending = true;
            for (int i = 0; i < count; i++) {
                final int index = i;
                playbackHandler.postDelayed(() -> {
                    if (ticket != debugHeldGeneration || playbackState == STATE_STOPPED) return;
                    nativeOnMediaControl(index % 2 == 0 ? CONTROL_NEXT : CONTROL_PREVIOUS, 0);
                    if (index == count - 1) {
                        debugMutationPending = false;
                        playbackHandler.postDelayed(() -> sendArtworkWorkerEvent(artworkWorker), 200);
                    }
                }, (long) index * interval);
            }
            return;
        }
        if ("rapid-skip".equals(command)) {
            int count = Math.max(1, Math.min(100, extras.getInt("count", 20)));
            for (int index = 0; index < count; index++) {
                nativeOnMediaControl(index % 2 == 0 ? CONTROL_NEXT : CONTROL_PREVIOUS, 0);
            }
            playbackHandler.postDelayed(() -> sendArtworkWorkerEvent(artworkWorker), 400);
            return;
        }
        if ("reverse".equals(command)) {
            nativeDebugReversePlaylist();
            playbackHandler.postDelayed(() -> sendArtworkWorkerEvent(artworkWorker), 200);
            return;
        }
        if ("reorder-replay".equals(command)) {
            Intent captured = new Intent(this, XmmsPlaybackService.class).setAction(ACTION_UPDATE)
                    .putExtra(EXTRA_STATE, playbackState).putExtra(EXTRA_TITLE, playbackTitle)
                    .putExtra(EXTRA_DURATION_MS, playbackDurationMs).putExtra(EXTRA_CURRENT_INDEX, currentMediaItemIndex)
                    .putExtra(EXTRA_PLAYLIST_SIZE, playlistSize).putExtra(EXTRA_PROJECTION_TOKEN, acceptedProjection);
            nativeDebugReversePlaylist();
            long ticket = ++debugHeldGeneration;
            playbackHandler.postDelayed(() -> {
                if (ticket == debugHeldGeneration && playbackState != STATE_STOPPED) {
                    onStartCommand(captured, 0, 0);
                    playbackHandler.postDelayed(() -> sendArtworkWorkerEvent(artworkWorker), 200);
                }
            }, Math.max(0, Math.min(2000, extras.getInt("projection_delay_ms", 400))));
            return;
        }
        if ("configure".equals(command)) {
            artworkWorker.debugSetRenderDelay(extras.getInt("delay_ms", 0));
            sendArtworkWorkerEvent(artworkWorker); return;
        }
        if ("stale".equals(command)) {
            applyNativePlaybackState(STATE_PLAYING, "STALE WRONG TITLE", 0, 0, 0, 0, 0, 0, 0, false, false, "obsolete-projection");
            playbackHandler.postDelayed(() -> sendArtworkWorkerEvent(artworkWorker), 200);
            return;
        }
        if ("cached".equals(command)) { artworkWorker.debugCachedRequest(); return; }
        if ("guard".equals(command)) { artworkWorker.debugMainThreadGuard(); return; }
        int count = Math.max(1, Math.min(100, extras.getInt("burst", 1)));
        for (int index = 0; index < count; index++) {
            artworkWorker.debugRequest(extras.getInt("delay_ms", 0), extras.getInt("failures", 0),
                    extras.getInt("delivery_delay_ms", 0));
        }
    }

    private String mediaId(long index) {
        return index < 0 ? null : MEDIA_ID_PREFIX + index;
    }

    private Uri iconUri() {
        return Uri.parse(
                "android.resource://" + getPackageName() + "/drawable/icon");
    }

    private Notification buildNotification(boolean playing) {
        return new Notification.Builder(this, CHANNEL_ID)
                .setSmallIcon(android.R.drawable.ic_media_play)
                .setLargeIcon(readyArtwork)
                .setContentTitle(playbackTitle)
                .setContentText(playing ? "XMMS Renascene - Playing" : "XMMS Renascene - Paused")
                .setTicker(playbackTitle)
                .setCategory(Notification.CATEGORY_TRANSPORT)
                .setVisibility(Notification.VISIBILITY_PUBLIC)
                .setOnlyAlertOnce(true)
                .setOngoing(true)
                .setShowWhen(false)
                .setContentIntent(activityPendingIntent())
                .setStyle(new Notification.MediaStyle()
                        .setMediaSession(mediaSession.getSessionToken()))
                .build();
    }

    private PendingIntent activityPendingIntent() {
        Intent intent = new Intent(this, XmmsActivity.class)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_SINGLE_TOP);
        return PendingIntent.getActivity(
                this,
                0,
                intent,
                PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
    }

    private boolean requestPlaybackAudioFocus() {
        if (audioFocusHeld) {
            return true;
        }
        if (audioManager == null || audioFocusRequest == null) {
            return false;
        }
        audioFocusHeld =
                audioManager.requestAudioFocus(audioFocusRequest)
                        == AudioManager.AUDIOFOCUS_REQUEST_GRANTED;
        return audioFocusHeld;
    }

    private boolean dispatchMediaControl(int control, long value) {
        boolean startsPlayback =
                control == CONTROL_PLAY
                        || control == CONTROL_NEXT
                        || control == CONTROL_PREVIOUS
                        || control == CONTROL_PLAY_MEDIA_ITEM;
        if (startsPlayback && !requestPlaybackAudioFocus()) {
            Log.w(TAG, "Rejecting playback control because audio focus was denied");
            return false;
        }
        nativeOnMediaControl(control, value);
        return true;
    }

    private void handleAudioFocusChange(int focusChange) {
        if (focusChange == AudioManager.AUDIOFOCUS_GAIN) {
            audioFocusHeld = true;
            if (resumeAfterFocusGain) {
                resumeAfterFocusGain = false;
                dispatchMediaControl(CONTROL_PLAY, 0);
            }
            return;
        }
        if (focusChange == AudioManager.AUDIOFOCUS_LOSS) {
            audioFocusHeld = false;
        }
        if (playbackState != STATE_PLAYING) {
            return;
        }
        resumeAfterFocusGain =
                focusChange == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT
                        || focusChange == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK;
        dispatchMediaControl(CONTROL_PAUSE, 0);
    }

    private void abandonAudioFocus() {
        resumeAfterFocusGain = false;
        if (audioFocusHeld && audioManager != null && audioFocusRequest != null) {
            audioFocusHeld = false;
            audioManager.abandonAudioFocusRequest(audioFocusRequest);
        }
    }

    private void updateWakeLock(boolean playing) {
        if (playing && !playbackWakeLock.isHeld()) {
            playbackWakeLock.acquire();
        } else if (!playing) {
            releaseWakeLock();
        }
    }

    private void releaseWakeLock() {
        if (playbackWakeLock != null && playbackWakeLock.isHeld()) {
            playbackWakeLock.release();
        }
    }

    private void stopPlaybackService() {
        closeArtworkWorker();
        if (browseArtwork != null) { browseArtwork.close(); browseArtwork = null; }
        // A bound MediaBrowser may retain this session after stopSelf: remove
        // its bitmap metadata as well as the foreground notification.
        if (mediaSession != null) publishMetadata();
        if (artworkDebuggable) sendArtworkWorkerEvent(artworkWorker);
        foregroundActive = false;
        debugHeldProjection = null; debugHeldGeneration++; debugMutationPending = false;
        lastMetadata = null; lastNotification = null; acceptedProjection = "";
        playbackState = STATE_STOPPED;
        XmmsPlayerWidget.updateAll(
                this,
                hasPrevious,
                hasNext);
        XmmsPlayerInfoWidget.updateAll(
                this,
                playbackState,
                playbackTitle,
                playbackBitrate,
                playbackFrequency,
                playbackChannels);
        releaseWakeLock();
        abandonAudioFocus();
        if (mediaSession != null) {
            mediaSession.setActive(false);
        }
        if (Build.VERSION.SDK_INT >= 24) {
            stopForeground(STOP_FOREGROUND_REMOVE);
        } else {
            stopForeground(true);
        }
        stopSelf();
    }
}
