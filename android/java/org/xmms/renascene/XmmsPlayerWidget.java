package org.xmms.renascene;

import android.app.PendingIntent;
import android.appwidget.AppWidgetManager;
import android.appwidget.AppWidgetProvider;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.res.Configuration;
import android.graphics.Bitmap;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.util.DisplayMetrics;
import android.widget.RemoteViews;

public final class XmmsPlayerWidget extends AppWidgetProvider {
    private static final int PLAYER_WIDTH = 114;
    private static final int PLAYER_HEIGHT = 18;
    private static final int NO_PRESSED_CONTROL = 0;
    private static final long PRESSED_DURATION_MS = 150;
    private static final String ACTION_CONTROL =
            "org.xmms.renascene.widget.CONTROL";
    private static final String EXTRA_CONTROL = "control";
    private static final String PREFERENCES = "xmms_player_widget";
    private static final String KEY_HAS_PREVIOUS = "hasPrevious";
    private static final String KEY_HAS_NEXT = "hasNext";
    private static final Object PRESSED_LOCK = new Object();
    private static final Handler PRESSED_HANDLER = new Handler(Looper.getMainLooper());
    private static long pressedGeneration;
    private static int pressedControl = NO_PRESSED_CONTROL;
    private static Runnable restorePressedRunnable;

    static {
        System.loadLibrary("xmms_renascene");
    }

    private static native int[] nativeRenderPlayerWidget(
            String filesDir,
            String cacheDir,
            int pressedControl);

    @Override
    public void onUpdate(Context context, AppWidgetManager manager, int[] widgetIds) {
        WidgetState state = loadState(context);
        synchronized (PRESSED_LOCK) {
            for (int widgetId : widgetIds) {
                updateWidget(
                        context,
                        manager,
                        widgetId,
                        manager.getAppWidgetOptions(widgetId),
                        state,
                        pressedControl);
            }
        }
    }

    @Override
    public void onAppWidgetOptionsChanged(
            Context context,
            AppWidgetManager manager,
            int widgetId,
            Bundle newOptions) {
        synchronized (PRESSED_LOCK) {
            updateWidget(
                    context,
                    manager,
                    widgetId,
                    newOptions,
                    loadState(context),
                    pressedControl);
        }
    }

    @Override
    public void onReceive(Context context, Intent intent) {
        super.onReceive(context, intent);
        if (!ACTION_CONTROL.equals(intent.getAction())) {
            return;
        }
        int control =
                intent.getIntExtra(EXTRA_CONTROL, XmmsPlaybackService.CONTROL_PLAY);
        showPressedControl(context, control);
        Intent serviceIntent = new Intent(context, XmmsPlaybackService.class)
                .setAction(XmmsPlaybackService.ACTION_WIDGET_CONTROL)
                .putExtra(XmmsPlaybackService.EXTRA_WIDGET_CONTROL, control);
        if (Build.VERSION.SDK_INT >= 26) {
            context.startForegroundService(serviceIntent);
        } else {
            context.startService(serviceIntent);
        }
    }

    @Override
    public void onDisabled(Context context) {
        synchronized (PRESSED_LOCK) {
            pressedGeneration++;
            pressedControl = NO_PRESSED_CONTROL;
            if (restorePressedRunnable != null) {
                PRESSED_HANDLER.removeCallbacks(restorePressedRunnable);
                restorePressedRunnable = null;
            }
        }
        super.onDisabled(context);
    }

    static void updateAll(
            Context context,
            boolean hasPrevious,
            boolean hasNext) {
        context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
                .edit()
                .putBoolean(KEY_HAS_PREVIOUS, hasPrevious)
                .putBoolean(KEY_HAS_NEXT, hasNext)
                .apply();

        AppWidgetManager manager = AppWidgetManager.getInstance(context);
        ComponentName provider = new ComponentName(context, XmmsPlayerWidget.class);
        int[] widgetIds = manager.getAppWidgetIds(provider);
        if (widgetIds.length == 0) {
            return;
        }
        WidgetState state = new WidgetState(hasPrevious, hasNext);
        synchronized (PRESSED_LOCK) {
            updateWidgets(context, manager, widgetIds, state, pressedControl);
        }
    }

    static void refreshAll(Context context) {
        Context applicationContext = context.getApplicationContext();
        AppWidgetManager manager = AppWidgetManager.getInstance(applicationContext);
        ComponentName provider =
                new ComponentName(applicationContext, XmmsPlayerWidget.class);
        int[] widgetIds = manager.getAppWidgetIds(provider);
        if (widgetIds.length == 0) {
            return;
        }
        WidgetState state = loadState(applicationContext);
        synchronized (PRESSED_LOCK) {
            updateWidgets(applicationContext, manager, widgetIds, state, pressedControl);
        }
    }

    private static void showPressedControl(Context context, int control) {
        Context applicationContext = context.getApplicationContext();
        AppWidgetManager manager = AppWidgetManager.getInstance(applicationContext);
        ComponentName provider =
                new ComponentName(applicationContext, XmmsPlayerWidget.class);
        int[] widgetIds = manager.getAppWidgetIds(provider);
        if (widgetIds.length == 0) {
            return;
        }
        WidgetState state = loadState(applicationContext);
        synchronized (PRESSED_LOCK) {
            long generation = ++pressedGeneration;
            pressedControl = control;
            if (restorePressedRunnable != null) {
                PRESSED_HANDLER.removeCallbacks(restorePressedRunnable);
            }
            updateWidgets(applicationContext, manager, widgetIds, state, control);
            restorePressedRunnable = () -> {
                synchronized (PRESSED_LOCK) {
                    if (generation != pressedGeneration) {
                        return;
                    }
                    pressedControl = NO_PRESSED_CONTROL;
                    restorePressedRunnable = null;
                    updateWidgets(
                            applicationContext,
                            manager,
                            manager.getAppWidgetIds(provider),
                            loadState(applicationContext),
                            NO_PRESSED_CONTROL);
                }
            };
            PRESSED_HANDLER.postDelayed(restorePressedRunnable, PRESSED_DURATION_MS);
        }
    }

    private static void updateWidgets(
            Context context,
            AppWidgetManager manager,
            int[] widgetIds,
            WidgetState state,
            int activePressedControl) {
        for (int widgetId : widgetIds) {
            updateWidget(
                    context,
                    manager,
                    widgetId,
                    manager.getAppWidgetOptions(widgetId),
                    state,
                    activePressedControl);
        }
    }

    private static void updateWidget(
            Context context,
            AppWidgetManager manager,
            int widgetId,
            Bundle options,
            WidgetState state,
            int activePressedControl) {
        manager.updateAppWidget(
                widgetId,
                remoteViews(context, options, state, activePressedControl));
    }

    private static RemoteViews remoteViews(
            Context context,
            Bundle options,
            WidgetState state,
            int activePressedControl) {
        String packageName = context.getPackageName();
        int layout = resourceId(context, "layout", "widget_player");
        int playerContainer = resourceId(context, "id", "widget_player_container");
        int playerImage = resourceId(context, "id", "widget_player_image");
        int previous = resourceId(context, "id", "widget_previous");
        int play = resourceId(context, "id", "widget_play");
        int pause = resourceId(context, "id", "widget_pause");
        int stop = resourceId(context, "id", "widget_stop");
        int next = resourceId(context, "id", "widget_next");
        RemoteViews views = new RemoteViews(packageName, layout);
        WidgetPadding padding = widgetPadding(context, options);
        views.setViewPadding(
                playerContainer,
                padding.left,
                padding.top,
                padding.right,
                padding.bottom);
        int[] pixels = nativeRenderPlayerWidget(
                context.getFilesDir().getAbsolutePath(),
                context.getCacheDir().getAbsolutePath(),
                activePressedControl);
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

    private static WidgetPadding widgetPadding(Context context, Bundle options) {
        boolean landscape =
                context.getResources().getConfiguration().orientation
                        == Configuration.ORIENTATION_LANDSCAPE;
        int widthDp = optionDimension(
                options,
                landscape
                        ? AppWidgetManager.OPTION_APPWIDGET_MAX_WIDTH
                        : AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH,
                PLAYER_WIDTH);
        int heightDp = optionDimension(
                options,
                landscape
                        ? AppWidgetManager.OPTION_APPWIDGET_MIN_HEIGHT
                        : AppWidgetManager.OPTION_APPWIDGET_MAX_HEIGHT,
                PLAYER_HEIGHT);
        DisplayMetrics metrics = context.getResources().getDisplayMetrics();
        int width = Math.max(1, Math.round(widthDp * metrics.density));
        int height = Math.max(1, Math.round(heightDp * metrics.density));
        int contentWidth = width;
        int contentHeight = Math.round((float) width * PLAYER_HEIGHT / PLAYER_WIDTH);
        if (contentHeight > height) {
            contentHeight = height;
            contentWidth = Math.round((float) height * PLAYER_WIDTH / PLAYER_HEIGHT);
        }
        int horizontalPadding = Math.max(0, width - contentWidth);
        int verticalPadding = Math.max(0, height - contentHeight);
        return new WidgetPadding(
                horizontalPadding / 2,
                verticalPadding / 2,
                horizontalPadding - horizontalPadding / 2,
                verticalPadding - verticalPadding / 2);
    }

    private static int optionDimension(Bundle options, String key, int fallback) {
        if (options == null) {
            return fallback;
        }
        int value = options.getInt(key, fallback);
        return value > 0 ? value : fallback;
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
                preferences.getBoolean(KEY_HAS_PREVIOUS, false),
                preferences.getBoolean(KEY_HAS_NEXT, false));
    }

    private static int resourceId(Context context, String type, String name) {
        return context.getResources().getIdentifier(name, type, context.getPackageName());
    }

    private static final class WidgetState {
        final boolean hasPrevious;
        final boolean hasNext;

        WidgetState(boolean hasPrevious, boolean hasNext) {
            this.hasPrevious = hasPrevious;
            this.hasNext = hasNext;
        }
    }

    private static final class WidgetPadding {
        final int left;
        final int top;
        final int right;
        final int bottom;

        WidgetPadding(int left, int top, int right, int bottom) {
            this.left = left;
            this.top = top;
            this.right = right;
            this.bottom = bottom;
        }
    }
}
