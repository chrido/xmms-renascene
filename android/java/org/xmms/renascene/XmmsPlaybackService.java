package org.xmms.renascene;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Intent;
import android.content.pm.ServiceInfo;
import android.media.AudioAttributes;
import android.media.AudioFocusRequest;
import android.media.AudioManager;
import android.media.MediaMetadata;
import android.media.session.MediaSession;
import android.media.session.PlaybackState;
import android.os.Build;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.os.PowerManager;

public final class XmmsPlaybackService extends Service {
    static final String ACTION_UPDATE = "org.xmms.renascene.service.UPDATE";
    static final String EXTRA_STATE = "state";
    static final String EXTRA_TITLE = "title";
    static final String EXTRA_DURATION_MS = "durationMs";
    static final String EXTRA_POSITION_MS = "positionMs";
    static final String EXTRA_HAS_PREVIOUS = "hasPrevious";
    static final String EXTRA_HAS_NEXT = "hasNext";

    private static final String CHANNEL_ID = "xmms_playback";
    private static final int NOTIFICATION_ID = 1;
    private static final int CONTROL_PAUSE = 1;
    private static final int CONTROL_PLAY = 2;
    private static final int CONTROL_NEXT = 3;
    private static final int CONTROL_PREVIOUS = 4;
    private static final int CONTROL_SEEK = 5;
    private static final int CONTROL_STOP = 6;

    static {
        System.loadLibrary("xmms_renascene");
    }

    private native void nativeOnMediaControl(int control, long value);
    private native void nativePollPlayback();

    private PowerManager.WakeLock playbackWakeLock;
    private NotificationManager notificationManager;
    private AudioManager audioManager;
    private AudioFocusRequest audioFocusRequest;
    private MediaSession mediaSession;
    private final Handler playbackHandler = new Handler(Looper.getMainLooper());
    private final Runnable playbackPoll = new Runnable() {
        @Override
        public void run() {
            if (playbackState == 1) {
                nativePollPlayback();
            }
            playbackHandler.postDelayed(this, 250);
        }
    };

    private int playbackState;
    private String playbackTitle = "XMMS Renascene";
    private long playbackDurationMs = -1;
    private long playbackPositionMs;
    private boolean hasPrevious;
    private boolean hasNext;

    @Override
    public void onCreate() {
        super.onCreate();
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
                .setOnAudioFocusChangeListener(focusChange -> {})
                .build();

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
        });
        playbackHandler.post(playbackPoll);
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        if (intent == null || !ACTION_UPDATE.equals(intent.getAction())) {
            stopPlaybackService();
            return START_NOT_STICKY;
        }
        applyNativePlaybackState(
                intent.getIntExtra(EXTRA_STATE, 0),
                intent.getStringExtra(EXTRA_TITLE),
                intent.getLongExtra(EXTRA_DURATION_MS, -1),
                intent.getLongExtra(EXTRA_POSITION_MS, 0),
                intent.getBooleanExtra(EXTRA_HAS_PREVIOUS, false),
                intent.getBooleanExtra(EXTRA_HAS_NEXT, false));
        return START_NOT_STICKY;
    }

    public void applyNativePlaybackState(
            int state,
            String title,
            long durationMs,
            long positionMs,
            boolean previous,
            boolean next) {
        playbackState = state;
        playbackTitle = title == null || title.isEmpty() ? "XMMS Renascene" : title;
        playbackDurationMs = durationMs;
        playbackPositionMs = Math.max(0, positionMs);
        hasPrevious = previous;
        hasNext = next;

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

    @Override
    public void onDestroy() {
        playbackHandler.removeCallbacks(playbackPoll);
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
    public IBinder onBind(Intent intent) {
        return null;
    }

    private void updateMediaSession(boolean playing) {
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
        MediaMetadata.Builder metadata = new MediaMetadata.Builder()
                .putString(MediaMetadata.METADATA_KEY_TITLE, playbackTitle)
                .putString(MediaMetadata.METADATA_KEY_DISPLAY_TITLE, playbackTitle);
        if (playbackDurationMs >= 0) {
            metadata.putLong(MediaMetadata.METADATA_KEY_DURATION, playbackDurationMs);
        }
        mediaSession.setMetadata(metadata.build());
        mediaSession.setPlaybackState(new PlaybackState.Builder()
                .setActions(actions)
                .setState(
                        playing ? PlaybackState.STATE_PLAYING : PlaybackState.STATE_PAUSED,
                        playbackPositionMs,
                        playing ? 1.0f : 0.0f)
                .build());
        mediaSession.setActive(true);
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

    private void abandonAudioFocus() {
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
