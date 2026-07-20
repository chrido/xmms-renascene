package org.xmms.renascene;

import android.app.PendingIntent;
import android.appwidget.AppWidgetManager;
import android.appwidget.AppWidgetProvider;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.graphics.Bitmap;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.widget.RemoteViews;

/**
 * App-widget process boundary for synchronous native player-information rendering.
 *
 * <p>The provider may run without an Activity. It reads atomically replaced persisted state;
 * process-local native skin/marquee caches are disposable accelerators, not authoritative state.
 */
public final class XmmsPlayerInfoWidget extends AppWidgetProvider {
    private static final int INFO_WIDTH = 164;
    private static final int INFO_HEIGHT = 37;
    private static final int FRAME_WIDTH = INFO_WIDTH + 4;
    private static final int FRAME_HEIGHT = INFO_HEIGHT + 4;
    private static final int OPEN_PLAYER_REQUEST_CODE = 1000;
    private static final long MARQUEE_TICK_MS = 250;
    private static final long MARQUEE_ACTIVE = 1L;
    private static final long MARQUEE_CHANGED = 2L;
    private static final int MARQUEE_OFFSET_SHIFT = 2;
    private static final String PREFERENCES = "xmms_player_info_widget";
    private static final String KEY_PLAYBACK_STATE = "playbackState";
    private static final String KEY_TITLE = "title";
    private static final String KEY_BITRATE = "bitrate";
    private static final String KEY_FREQUENCY = "frequency";
    private static final String KEY_CHANNELS = "channels";
    private static final Handler MARQUEE_HANDLER = new Handler(Looper.getMainLooper());
    private static Context marqueeContext;
    private static long marqueeLastTickMs;
    private static final Runnable MARQUEE_TICK = new Runnable() {
        @Override
        public void run() {
            Context context;
            long elapsedMs;
            synchronized (XmmsPlayerInfoWidget.class) {
                context = marqueeContext;
                if (context == null) {
                    return;
                }
                long now = SystemClock.elapsedRealtime();
                elapsedMs = Math.max(0, now - marqueeLastTickMs);
                marqueeLastTickMs = now;
            }

            WidgetState state = loadState(context);
            long marquee = nativeUpdateTitleMarquee(
                    state.title, state.playbackState, elapsedMs);
            if (!marqueeActive(marquee)) {
                stopMarquee();
                return;
            }
            if (marqueeChanged(marquee)) {
                AppWidgetManager manager = AppWidgetManager.getInstance(context);
                ComponentName provider = new ComponentName(context, XmmsPlayerInfoWidget.class);
                int[] widgetIds = manager.getAppWidgetIds(provider);
                if (widgetIds.length == 0) {
                    stopMarquee();
                    return;
                }
                updateWidgets(context, manager, widgetIds, state, marqueeOffset(marquee));
            }
            synchronized (XmmsPlayerInfoWidget.class) {
                if (marqueeContext != null) {
                    MARQUEE_HANDLER.postDelayed(this, MARQUEE_TICK_MS);
                }
            }
        }
    };

    static {
        System.loadLibrary("xmms_renascene");
    }

    private static native int[] nativeRenderPlayerInfoWidget(
            String filesDir,
            String cacheDir,
            String title,
            int bitrate,
            int frequency,
            int channels,
            int titleOffsetPx);

    private static native long nativeUpdateTitleMarquee(
            String title,
            int playbackState,
            long elapsedMs);

    @Override
    public void onUpdate(Context context, AppWidgetManager manager, int[] widgetIds) {
        WidgetState state = loadState(context);
        updateWidgetsAndSchedule(context, manager, widgetIds, state);
    }

    @Override
    public void onAppWidgetOptionsChanged(
            Context context,
            AppWidgetManager manager,
            int widgetId,
            Bundle newOptions) {
        WidgetState state = loadState(context);
        long marquee = nativeUpdateTitleMarquee(state.title, state.playbackState, 0);
        updateWidget(context, manager, widgetId, newOptions, state, marqueeOffset(marquee));
        setMarqueeActive(context, marqueeActive(marquee));
    }

    @Override
    public void onDisabled(Context context) {
        stopMarquee();
    }

    static void updateAll(
            Context context,
            int playbackState,
            String title,
            int bitrate,
            int frequency,
            int channels) {
        Context applicationContext = context.getApplicationContext();
        WidgetState state =
                new WidgetState(playbackState, title, bitrate, frequency, channels);
        applicationContext.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
                .edit()
                .putInt(KEY_PLAYBACK_STATE, state.playbackState)
                .putString(KEY_TITLE, state.title)
                .putInt(KEY_BITRATE, state.bitrate)
                .putInt(KEY_FREQUENCY, state.frequency)
                .putInt(KEY_CHANNELS, state.channels)
                .apply();

        AppWidgetManager manager = AppWidgetManager.getInstance(applicationContext);
        ComponentName provider =
                new ComponentName(applicationContext, XmmsPlayerInfoWidget.class);
        int[] widgetIds = manager.getAppWidgetIds(provider);
        if (widgetIds.length == 0) {
            stopMarquee();
            return;
        }
        updateWidgetsAndSchedule(applicationContext, manager, widgetIds, state);
    }

    static void refreshAll(Context context) {
        Context applicationContext = context.getApplicationContext();
        AppWidgetManager manager = AppWidgetManager.getInstance(applicationContext);
        ComponentName provider =
                new ComponentName(applicationContext, XmmsPlayerInfoWidget.class);
        int[] widgetIds = manager.getAppWidgetIds(provider);
        if (widgetIds.length == 0) {
            stopMarquee();
            return;
        }
        updateWidgetsAndSchedule(
                applicationContext, manager, widgetIds, loadState(applicationContext));
    }

    private static void updateWidgetsAndSchedule(
            Context context,
            AppWidgetManager manager,
            int[] widgetIds,
            WidgetState state) {
        long marquee = nativeUpdateTitleMarquee(state.title, state.playbackState, 0);
        updateWidgets(context, manager, widgetIds, state, marqueeOffset(marquee));
        setMarqueeActive(context, marqueeActive(marquee));
    }

    private static void updateWidgets(
            Context context,
            AppWidgetManager manager,
            int[] widgetIds,
            WidgetState state,
            int titleOffsetPx) {
        for (int widgetId : widgetIds) {
            updateWidget(
                    context,
                    manager,
                    widgetId,
                    manager.getAppWidgetOptions(widgetId),
                    state,
                    titleOffsetPx);
        }
    }

    private static void updateWidget(
            Context context,
            AppWidgetManager manager,
            int widgetId,
            Bundle options,
            WidgetState state,
            int titleOffsetPx) {
        String packageName = context.getPackageName();
        int layout = resourceId(context, "layout", "widget_player_info");
        int content = resourceId(context, "id", "widget_player_info_content");
        int image = resourceId(context, "id", "widget_player_info_image");
        int open = resourceId(context, "id", "widget_player_info_open");
        RemoteViews views = new RemoteViews(packageName, layout);
        XmmsWidgetSupport.WidgetPadding padding =
                XmmsWidgetSupport.proportionalPadding(
                        context, options, FRAME_WIDTH, FRAME_HEIGHT);
        views.setViewPadding(
                content,
                padding.left,
                padding.top,
                padding.right,
                padding.bottom);
        int[] pixels = nativeRenderPlayerInfoWidget(
                context.getFilesDir().getAbsolutePath(),
                context.getCacheDir().getAbsolutePath(),
                state.title,
                state.bitrate,
                state.frequency,
                state.channels,
                titleOffsetPx);
        if (pixels != null && pixels.length == INFO_WIDTH * INFO_HEIGHT) {
            views.setImageViewBitmap(
                    image,
                    Bitmap.createBitmap(
                            pixels,
                            INFO_WIDTH,
                            INFO_HEIGHT,
                            Bitmap.Config.ARGB_8888));
        }
        views.setOnClickPendingIntent(open, openPlayerPendingIntent(context));
        manager.updateAppWidget(widgetId, views);
    }

    private static PendingIntent openPlayerPendingIntent(Context context) {
        Intent intent = new Intent(context, XmmsActivity.class)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_SINGLE_TOP);
        return PendingIntent.getActivity(
                context,
                OPEN_PLAYER_REQUEST_CODE,
                intent,
                PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
    }

    private static WidgetState loadState(Context context) {
        SharedPreferences preferences =
                context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE);
        return new WidgetState(
                preferences.getInt(KEY_PLAYBACK_STATE, 0),
                preferences.getString(KEY_TITLE, "XMMS Renascene"),
                preferences.getInt(KEY_BITRATE, 0),
                preferences.getInt(KEY_FREQUENCY, 0),
                preferences.getInt(KEY_CHANNELS, 0));
    }

    private static synchronized void setMarqueeActive(Context context, boolean active) {
        MARQUEE_HANDLER.removeCallbacks(MARQUEE_TICK);
        if (!active) {
            marqueeContext = null;
            return;
        }
        marqueeContext = context.getApplicationContext();
        marqueeLastTickMs = SystemClock.elapsedRealtime();
        MARQUEE_HANDLER.postDelayed(MARQUEE_TICK, MARQUEE_TICK_MS);
    }

    private static synchronized void stopMarquee() {
        MARQUEE_HANDLER.removeCallbacks(MARQUEE_TICK);
        marqueeContext = null;
    }

    private static boolean marqueeActive(long marquee) {
        return (marquee & MARQUEE_ACTIVE) != 0;
    }

    private static boolean marqueeChanged(long marquee) {
        return (marquee & MARQUEE_CHANGED) != 0;
    }

    private static int marqueeOffset(long marquee) {
        return (int) (marquee >>> MARQUEE_OFFSET_SHIFT);
    }

    private static int resourceId(Context context, String type, String name) {
        return context.getResources().getIdentifier(name, type, context.getPackageName());
    }

    private static final class WidgetState {
        final int playbackState;
        final String title;
        final int bitrate;
        final int frequency;
        final int channels;

        WidgetState(
                int playbackState,
                String title,
                int bitrate,
                int frequency,
                int channels) {
            this.playbackState = Math.max(0, Math.min(2, playbackState));
            this.title = title == null || title.isEmpty() ? "XMMS Renascene" : title;
            this.bitrate = Math.max(0, bitrate);
            this.frequency = Math.max(0, frequency);
            this.channels = Math.max(0, channels);
        }
    }
}
