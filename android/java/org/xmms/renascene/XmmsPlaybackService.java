package org.xmms.renascene;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.pm.ServiceInfo;
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
import android.service.media.MediaBrowserService;
import android.util.Log;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Locale;

public final class XmmsPlaybackService extends MediaBrowserService {
    static final String ACTION_UPDATE = "org.xmms.renascene.service.UPDATE";
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

    private PowerManager.WakeLock playbackWakeLock;
    private NotificationManager notificationManager;
    private AudioManager audioManager;
    private AudioFocusRequest audioFocusRequest;
    private MediaSession mediaSession;
    private boolean resumeAfterFocusGain;
    private boolean noisyReceiverRegistered;
    private final BroadcastReceiver noisyReceiver = new BroadcastReceiver() {
        @Override
        public void onReceive(Context context, Intent intent) {
            if (AudioManager.ACTION_AUDIO_BECOMING_NOISY.equals(intent.getAction())
                    && playbackState == 1) {
                resumeAfterFocusGain = false;
                nativeOnMediaControl(CONTROL_PAUSE, 0);
            }
        }
    };
    private final Handler playbackHandler = new Handler(Looper.getMainLooper());
    private final Runnable playbackPoll = new Runnable() {
        @Override
        public void run() {
            if (playbackState != 0) {
                nativePollPlayback();
            }
            playbackHandler.postDelayed(this, 250);
        }
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
    private boolean hasPrevious;
    private boolean hasNext;

    @Override
    public void onCreate() {
        super.onCreate();
        nativeInitializeMediaLibrary(
                getFilesDir().getAbsolutePath(), getCacheDir().getAbsolutePath());
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
                nativeOnMediaControl(CONTROL_PLAY, 0);
            }

            @Override
            public void onPause() {
                nativeOnMediaControl(CONTROL_PAUSE, 0);
            }

            @Override
            public void onSkipToNext() {
                nativeOnMediaControl(CONTROL_NEXT, 0);
            }

            @Override
            public void onSkipToPrevious() {
                nativeOnMediaControl(CONTROL_PREVIOUS, 0);
            }

            @Override
            public void onSeekTo(long positionMs) {
                nativeOnMediaControl(CONTROL_SEEK, positionMs);
            }

            @Override
            public void onStop() {
                nativeOnMediaControl(CONTROL_STOP, 0);
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
        });
        setSessionToken(mediaSession.getSessionToken());
        refreshMediaQueue();
        playbackHandler.post(playbackPoll);
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        if (intent != null && ACTION_WIDGET_CONTROL.equals(intent.getAction())) {
            if (Build.VERSION.SDK_INT >= 26) {
                startForeground(NOTIFICATION_ID, buildNotification(false));
            }
            nativeOnMediaControl(
                    intent.getIntExtra(EXTRA_WIDGET_CONTROL, CONTROL_PLAY), 0);
            playbackHandler.postDelayed(() -> {
                if (playbackState == 0) {
                    stopPlaybackService();
                }
            }, 1500);
            return START_NOT_STICKY;
        }
        if (intent == null || !ACTION_UPDATE.equals(intent.getAction())) {
            stopPlaybackService();
            return START_NOT_STICKY;
        }
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
                intent.getBooleanExtra(EXTRA_HAS_NEXT, false));
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
            boolean next) {
        String normalizedTitle =
                title == null || title.isEmpty() ? "XMMS Renascene" : title;
        int normalizedBitrate = Math.max(0, bitrate);
        int normalizedFrequency = Math.max(0, frequency);
        int normalizedChannels = Math.max(0, channels);
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
        XmmsPlayerWidget.updateAll(
                this,
                hasPrevious,
                hasNext);
        if (infoChanged) {
            XmmsPlayerInfoWidget.updateAll(
                    this,
                    playbackTitle,
                    playbackBitrate,
                    playbackFrequency,
                    playbackChannels);
        }
        refreshMediaQueue();
        notifyChildrenChanged(PLAYLIST_ID);

        if (state == 0) {
            stopPlaybackService();
            return;
        }
        boolean playing = state == 1;
        updateWakeLock(playing);
        updateAudioFocus(playing);
        updateMediaSession(playing);
        Notification notification = buildNotification(playing);
        if (Build.VERSION.SDK_INT >= 29) {
            startForeground(
                    NOTIFICATION_ID,
                    notification,
                    ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK);
        } else {
            startForeground(NOTIFICATION_ID, notification);
        }
    }

    public void applyNativePlaybackPosition(long positionMs) {
        playbackPositionMs = Math.max(0, positionMs);
        updatePlaybackState(playbackState == 1);
    }

    @Override
    public void onDestroy() {
        playbackHandler.removeCallbacks(playbackPoll);
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
        result.sendResult(mediaItems(start, Math.min(count, start + pageSize)));
    }

    private void updateMediaSession(boolean playing) {
        MediaMetadata.Builder metadata = new MediaMetadata.Builder()
                .putString(MediaMetadata.METADATA_KEY_TITLE, playbackTitle)
                .putString(MediaMetadata.METADATA_KEY_DISPLAY_TITLE, playbackTitle)
                .putString(MediaMetadata.METADATA_KEY_MEDIA_ID, mediaId(currentMediaItemIndex));
        if (playbackDurationMs >= 0) {
            metadata.putLong(MediaMetadata.METADATA_KEY_DURATION, playbackDurationMs);
        }
        mediaSession.setMetadata(metadata.build());
        updatePlaybackState(playing);
        mediaSession.setActive(true);
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
        int count = nativeMediaItemCount();
        playlistSize = count;
        List<MediaSession.QueueItem> queue = new ArrayList<>(count);
        for (int index = 0; index < count; index++) {
            queue.add(new MediaSession.QueueItem(mediaDescription(index), index));
        }
        mediaSession.setQueue(queue);
        mediaSession.setQueueTitle("Current playlist");
        long nativeIndex = nativeCurrentMediaItemIndex();
        if (nativeIndex >= 0) {
            currentMediaItemIndex = nativeIndex;
        }
    }

    private List<MediaBrowser.MediaItem> mediaItems(int start, int end) {
        List<MediaBrowser.MediaItem> items = new ArrayList<>(Math.max(0, end - start));
        for (int index = start; index < end; index++) {
            items.add(new MediaBrowser.MediaItem(
                    mediaDescription(index),
                    MediaBrowser.MediaItem.FLAG_PLAYABLE));
        }
        return items;
    }

    private MediaDescription mediaDescription(int index) {
        String title = nativeMediaItemTitle(index);
        if (title == null || title.isEmpty()) {
            title = "Track " + (index + 1);
        }
        Bundle extras = new Bundle();
        long durationMs = nativeMediaItemDurationMs(index);
        if (durationMs >= 0) {
            extras.putLong(MediaMetadata.METADATA_KEY_DURATION, durationMs);
        }
        return new MediaDescription.Builder()
                .setMediaId(mediaId(index))
                .setTitle(title)
                .setSubtitle("XMMS Renascene")
                .setIconUri(iconUri())
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
        nativeOnMediaControl(CONTROL_PLAY_MEDIA_ITEM, index);
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

    private void updateAudioFocus(boolean playing) {
        if (playing && audioManager != null && audioFocusRequest != null) {
            audioManager.requestAudioFocus(audioFocusRequest);
        }
    }

    private void handleAudioFocusChange(int focusChange) {
        if (focusChange == AudioManager.AUDIOFOCUS_GAIN) {
            if (resumeAfterFocusGain) {
                resumeAfterFocusGain = false;
                nativeOnMediaControl(CONTROL_PLAY, 0);
            }
            return;
        }
        if (playbackState != 1) {
            return;
        }
        resumeAfterFocusGain =
                focusChange == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT
                        || focusChange == AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK;
        nativeOnMediaControl(CONTROL_PAUSE, 0);
    }

    private void abandonAudioFocus() {
        resumeAfterFocusGain = false;
        if (audioManager != null && audioFocusRequest != null) {
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
        playbackState = 0;
        XmmsPlayerWidget.updateAll(
                this,
                hasPrevious,
                hasNext);
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
