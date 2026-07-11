package org.xmms.renascene;

import android.app.PendingIntent;
import android.appwidget.AppWidgetManager;
import android.appwidget.AppWidgetProvider;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.graphics.Bitmap;
import android.os.Build;
import android.widget.RemoteViews;

public final class XmmsPlayerWidget extends AppWidgetProvider {
    private static final int PLAYER_WIDTH = 275;
    private static final int PLAYER_HEIGHT = 116;
    private static final String ACTION_CONTROL =
            "org.xmms.renascene.widget.CONTROL";
    private static final String EXTRA_CONTROL = "control";
    private static final String PREFERENCES = "xmms_player_widget";
    private static final String KEY_STATE = "state";
    private static final String KEY_TITLE = "title";
    private static final String KEY_DURATION_MS = "durationMs";
    private static final String KEY_POSITION_MS = "positionMs";
    private static final String KEY_HAS_PREVIOUS = "hasPrevious";
    private static final String KEY_HAS_NEXT = "hasNext";

    static {
        System.loadLibrary("xmms_renascene");
    }

    private static native int[] nativeRenderPlayerWidget(
            String filesDir,
            String cacheDir,
            int state,
            String title,
            long durationMs,
            long positionMs);

    @Override
    public void onUpdate(Context context, AppWidgetManager manager, int[] widgetIds) {
        WidgetState state = loadState(context);
        for (int widgetId : widgetIds) {
            manager.updateAppWidget(widgetId, remoteViews(context, state));
        }
    }

    @Override
    public void onReceive(Context context, Intent intent) {
        super.onReceive(context, intent);
        if (!ACTION_CONTROL.equals(intent.getAction())) {
            return;
        }
        Intent serviceIntent = new Intent(context, XmmsPlaybackService.class)
                .setAction(XmmsPlaybackService.ACTION_WIDGET_CONTROL)
                .putExtra(
                        XmmsPlaybackService.EXTRA_WIDGET_CONTROL,
                        intent.getIntExtra(EXTRA_CONTROL, XmmsPlaybackService.CONTROL_PLAY));
        if (Build.VERSION.SDK_INT >= 26) {
            context.startForegroundService(serviceIntent);
        } else {
            context.startService(serviceIntent);
        }
    }

    static void updateAll(
            Context context,
            int state,
            String title,
            long durationMs,
            long positionMs,
            boolean hasPrevious,
            boolean hasNext) {
        String displayTitle =
                title == null || title.isEmpty() ? "XMMS Renascene" : title;
        context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
                .edit()
                .putInt(KEY_STATE, state)
                .putString(KEY_TITLE, displayTitle)
                .putLong(KEY_DURATION_MS, durationMs)
                .putLong(KEY_POSITION_MS, positionMs)
                .putBoolean(KEY_HAS_PREVIOUS, hasPrevious)
                .putBoolean(KEY_HAS_NEXT, hasNext)
                .apply();

        AppWidgetManager manager = AppWidgetManager.getInstance(context);
        ComponentName provider = new ComponentName(context, XmmsPlayerWidget.class);
        int[] widgetIds = manager.getAppWidgetIds(provider);
        if (widgetIds.length == 0) {
            return;
        }
        manager.updateAppWidget(
                widgetIds,
                remoteViews(
                        context,
                        new WidgetState(
                                state,
                                displayTitle,
                                durationMs,
                                positionMs,
                                hasPrevious,
                                hasNext)));
    }

    private static RemoteViews remoteViews(Context context, WidgetState state) {
        String packageName = context.getPackageName();
        int layout = resourceId(context, "layout", "widget_player");
        int playerImage = resourceId(context, "id", "widget_player_image");
        int previous = resourceId(context, "id", "widget_previous");
        int play = resourceId(context, "id", "widget_play");
        int pause = resourceId(context, "id", "widget_pause");
        int stop = resourceId(context, "id", "widget_stop");
        int next = resourceId(context, "id", "widget_next");
        RemoteViews views = new RemoteViews(packageName, layout);
        int[] pixels = nativeRenderPlayerWidget(
                context.getFilesDir().getAbsolutePath(),
                context.getCacheDir().getAbsolutePath(),
                state.state,
                state.title,
                state.durationMs,
                state.positionMs);
        if (pixels != null && pixels.length == PLAYER_WIDTH * PLAYER_HEIGHT) {
            views.setImageViewBitmap(
                    playerImage,
                    Bitmap.createBitmap(
                            pixels,
                            PLAYER_WIDTH,
                            PLAYER_HEIGHT,
                            Bitmap.Config.ARGB_8888));
        }
        views.setBoolean(previous, "setEnabled", state.hasPrevious);
        views.setBoolean(next, "setEnabled", state.hasNext);
        views.setOnClickPendingIntent(playerImage, activityPendingIntent(context));
        views.setOnClickPendingIntent(
                previous,
                controlPendingIntent(
                        context, XmmsPlaybackService.CONTROL_PREVIOUS));
        views.setOnClickPendingIntent(
                play,
                controlPendingIntent(context, XmmsPlaybackService.CONTROL_PLAY));
        views.setOnClickPendingIntent(
                pause,
                controlPendingIntent(context, XmmsPlaybackService.CONTROL_PAUSE));
        views.setOnClickPendingIntent(
                stop,
                controlPendingIntent(context, XmmsPlaybackService.CONTROL_STOP));
        views.setOnClickPendingIntent(
                next,
                controlPendingIntent(context, XmmsPlaybackService.CONTROL_NEXT));
        return views;
    }

    private static PendingIntent activityPendingIntent(Context context) {
        Intent intent = new Intent(context, XmmsActivity.class)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_SINGLE_TOP);
        return PendingIntent.getActivity(
                context,
                0,
                intent,
                PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
    }

    private static PendingIntent controlPendingIntent(Context context, int control) {
        Intent intent = new Intent(context, XmmsPlayerWidget.class)
                .setAction(ACTION_CONTROL)
                .putExtra(EXTRA_CONTROL, control);
        return PendingIntent.getBroadcast(
                context,
                control,
                intent,
                PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
    }

    private static WidgetState loadState(Context context) {
        SharedPreferences preferences =
                context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE);
        return new WidgetState(
                preferences.getInt(KEY_STATE, 0),
                preferences.getString(KEY_TITLE, "XMMS Renascene"),
                preferences.getLong(KEY_DURATION_MS, -1),
                preferences.getLong(KEY_POSITION_MS, 0),
                preferences.getBoolean(KEY_HAS_PREVIOUS, false),
                preferences.getBoolean(KEY_HAS_NEXT, false));
    }

    private static int resourceId(Context context, String type, String name) {
        return context.getResources().getIdentifier(name, type, context.getPackageName());
    }

    private static final class WidgetState {
        final int state;
        final String title;
        final long durationMs;
        final long positionMs;
        final boolean hasPrevious;
        final boolean hasNext;

        WidgetState(
                int state,
                String title,
                long durationMs,
                long positionMs,
                boolean hasPrevious,
                boolean hasNext) {
            this.state = state;
            this.title = title;
            this.durationMs = durationMs;
            this.positionMs = positionMs;
            this.hasPrevious = hasPrevious;
            this.hasNext = hasNext;
        }
    }
}
