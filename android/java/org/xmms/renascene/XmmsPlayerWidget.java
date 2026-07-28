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
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.widget.RemoteViews;

/**
 * App-widget process boundary for transport controls and synchronous native bitmap rendering.
 *
 * <p>The provider may run without an Activity. Control intents are forwarded to
 * {@link XmmsPlaybackService}; rendering reads atomically replaced persisted state and does not
 * depend on Activity-owned objects.
 */
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
    private static int cachedBitmapPressedControl = Integer.MIN_VALUE;
    private static Bitmap cachedBitmap;
    private static ResourceIds cachedResourceIds;

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
            cachedBitmap = null;
            cachedBitmapPressedControl = Integer.MIN_VALUE;
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
            cachedBitmap = null;
            cachedBitmapPressedControl = Integer.MIN_VALUE;
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
        ResourceIds resources = resourceIds(context);
        RemoteViews views = new RemoteViews(packageName, resources.layout);
        WidgetPadding padding = widgetPadding(context, options);
        views.setViewPadding(
                resources.playerContainer,
                padding.left,
                padding.top,
                padding.right,
                padding.bottom);
        Bitmap bitmap = playerBitmap(context, activePressedControl);
        if (bitmap != null) {
            views.setImageViewBitmap(resources.playerImage, bitmap);
        }
        views.setBoolean(resources.previous, "setEnabled", state.hasPrevious);
        views.setBoolean(resources.next, "setEnabled", state.hasNext);
        views.setOnClickPendingIntent(
                resources.previous,
                controlPendingIntent(
                        context, XmmsPlaybackService.CONTROL_PREVIOUS));
        views.setOnClickPendingIntent(
                resources.play,
                controlPendingIntent(context, XmmsPlaybackService.CONTROL_PLAY));
        views.setOnClickPendingIntent(
                resources.pause,
                controlPendingIntent(context, XmmsPlaybackService.CONTROL_PAUSE));
        views.setOnClickPendingIntent(
                resources.stop,
                controlPendingIntent(context, XmmsPlaybackService.CONTROL_STOP));
        views.setOnClickPendingIntent(
                resources.next,
                controlPendingIntent(context, XmmsPlaybackService.CONTROL_NEXT));
        return views;
    }

    private static Bitmap playerBitmap(Context context, int activePressedControl) {
        if (cachedBitmap != null && cachedBitmapPressedControl == activePressedControl) {
            return cachedBitmap;
        }
        int[] pixels = nativeRenderPlayerWidget(
                context.getFilesDir().getAbsolutePath(),
                context.getCacheDir().getAbsolutePath(),
                activePressedControl);
        if (pixels == null || pixels.length != PLAYER_WIDTH * PLAYER_HEIGHT) {
            return null;
        }
        cachedBitmap = Bitmap.createBitmap(
                pixels,
                PLAYER_WIDTH,
                PLAYER_HEIGHT,
                Bitmap.Config.ARGB_8888);
        cachedBitmapPressedControl = activePressedControl;
        return cachedBitmap;
    }

    private static WidgetPadding widgetPadding(Context context, Bundle options) {
        XmmsWidgetSupport.WidgetPadding padding =
                XmmsWidgetSupport.proportionalPadding(
                        context, options, PLAYER_WIDTH, PLAYER_HEIGHT);
        return new WidgetPadding(
                padding.left, padding.top, padding.right, padding.bottom);
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

    private static synchronized ResourceIds resourceIds(Context context) {
        if (cachedResourceIds == null) {
            cachedResourceIds = new ResourceIds(context);
        }
        return cachedResourceIds;
    }

    private static final class ResourceIds {
        final int layout;
        final int playerContainer;
        final int playerImage;
        final int previous;
        final int play;
        final int pause;
        final int stop;
        final int next;

        ResourceIds(Context context) {
            String packageName = context.getPackageName();
            layout = context.getResources().getIdentifier("widget_player", "layout", packageName);
            playerContainer = context.getResources().getIdentifier(
                    "widget_player_container", "id", packageName);
            playerImage = context.getResources().getIdentifier(
                    "widget_player_image", "id", packageName);
            previous = context.getResources().getIdentifier("widget_previous", "id", packageName);
            play = context.getResources().getIdentifier("widget_play", "id", packageName);
            pause = context.getResources().getIdentifier("widget_pause", "id", packageName);
            stop = context.getResources().getIdentifier("widget_stop", "id", packageName);
            next = context.getResources().getIdentifier("widget_next", "id", packageName);
        }
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
