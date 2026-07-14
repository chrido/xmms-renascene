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
import android.widget.RemoteViews;

public final class XmmsPlayerInfoWidget extends AppWidgetProvider {
    private static final int INFO_WIDTH = 157;
    private static final int INFO_HEIGHT = 26;
    private static final int OPEN_PLAYER_REQUEST_CODE = 1000;
    private static final String PREFERENCES = "xmms_player_info_widget";
    private static final String KEY_TITLE = "title";
    private static final String KEY_BITRATE = "bitrate";
    private static final String KEY_FREQUENCY = "frequency";
    private static final String KEY_CHANNELS = "channels";

    static {
        System.loadLibrary("xmms_renascene");
    }

    private static native int[] nativeRenderPlayerInfoWidget(
            String filesDir,
            String cacheDir,
            String title,
            int bitrate,
            int frequency,
            int channels);

    @Override
    public void onUpdate(Context context, AppWidgetManager manager, int[] widgetIds) {
        WidgetState state = loadState(context);
        updateWidgets(context, manager, widgetIds, state);
    }

    @Override
    public void onAppWidgetOptionsChanged(
            Context context,
            AppWidgetManager manager,
            int widgetId,
            Bundle newOptions) {
        updateWidget(context, manager, widgetId, newOptions, loadState(context));
    }

    static void updateAll(
            Context context,
            String title,
            int bitrate,
            int frequency,
            int channels) {
        Context applicationContext = context.getApplicationContext();
        WidgetState state = new WidgetState(title, bitrate, frequency, channels);
        applicationContext.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
                .edit()
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
            return;
        }
        updateWidgets(applicationContext, manager, widgetIds, state);
    }

    static void refreshAll(Context context) {
        Context applicationContext = context.getApplicationContext();
        AppWidgetManager manager = AppWidgetManager.getInstance(applicationContext);
        ComponentName provider =
                new ComponentName(applicationContext, XmmsPlayerInfoWidget.class);
        int[] widgetIds = manager.getAppWidgetIds(provider);
        if (widgetIds.length == 0) {
            return;
        }
        updateWidgets(applicationContext, manager, widgetIds, loadState(applicationContext));
    }

    private static void updateWidgets(
            Context context,
            AppWidgetManager manager,
            int[] widgetIds,
            WidgetState state) {
        for (int widgetId : widgetIds) {
            updateWidget(
                    context,
                    manager,
                    widgetId,
                    manager.getAppWidgetOptions(widgetId),
                    state);
        }
    }

    private static void updateWidget(
            Context context,
            AppWidgetManager manager,
            int widgetId,
            Bundle options,
            WidgetState state) {
        String packageName = context.getPackageName();
        int layout = resourceId(context, "layout", "widget_player_info");
        int container = resourceId(context, "id", "widget_player_info_container");
        int image = resourceId(context, "id", "widget_player_info_image");
        RemoteViews views = new RemoteViews(packageName, layout);
        XmmsWidgetSupport.WidgetPadding padding =
                XmmsWidgetSupport.proportionalPadding(
                        context, options, INFO_WIDTH, INFO_HEIGHT);
        views.setViewPadding(
                container, padding.left, padding.top, padding.right, padding.bottom);
        int[] pixels = nativeRenderPlayerInfoWidget(
                context.getFilesDir().getAbsolutePath(),
                context.getCacheDir().getAbsolutePath(),
                state.title,
                state.bitrate,
                state.frequency,
                state.channels);
        if (pixels != null && pixels.length == INFO_WIDTH * INFO_HEIGHT) {
            views.setImageViewBitmap(
                    image,
                    Bitmap.createBitmap(
                            pixels,
                            INFO_WIDTH,
                            INFO_HEIGHT,
                            Bitmap.Config.ARGB_8888));
        }
        views.setOnClickPendingIntent(container, openPlayerPendingIntent(context));
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
                preferences.getString(KEY_TITLE, "XMMS Renascene"),
                preferences.getInt(KEY_BITRATE, 0),
                preferences.getInt(KEY_FREQUENCY, 0),
                preferences.getInt(KEY_CHANNELS, 0));
    }

    private static int resourceId(Context context, String type, String name) {
        return context.getResources().getIdentifier(name, type, context.getPackageName());
    }

    private static final class WidgetState {
        final String title;
        final int bitrate;
        final int frequency;
        final int channels;

        WidgetState(String title, int bitrate, int frequency, int channels) {
            this.title = title == null || title.isEmpty() ? "XMMS Renascene" : title;
            this.bitrate = Math.max(0, bitrate);
            this.frequency = Math.max(0, frequency);
            this.channels = Math.max(0, channels);
        }
    }
}
