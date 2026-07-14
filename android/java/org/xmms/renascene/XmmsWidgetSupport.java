package org.xmms.renascene;

import android.appwidget.AppWidgetManager;
import android.content.Context;
import android.content.res.Configuration;
import android.os.Bundle;
import android.util.DisplayMetrics;

final class XmmsWidgetSupport {
    private XmmsWidgetSupport() {}

    static WidgetPadding proportionalPadding(
            Context context,
            Bundle options,
            int nativeWidth,
            int nativeHeight) {
        boolean landscape =
                context.getResources().getConfiguration().orientation
                        == Configuration.ORIENTATION_LANDSCAPE;
        int widthDp = optionDimension(
                options,
                landscape
                        ? AppWidgetManager.OPTION_APPWIDGET_MAX_WIDTH
                        : AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH,
                nativeWidth);
        int heightDp = optionDimension(
                options,
                landscape
                        ? AppWidgetManager.OPTION_APPWIDGET_MIN_HEIGHT
                        : AppWidgetManager.OPTION_APPWIDGET_MAX_HEIGHT,
                nativeHeight);
        DisplayMetrics metrics = context.getResources().getDisplayMetrics();
        int width = Math.max(1, Math.round(widthDp * metrics.density));
        int height = Math.max(1, Math.round(heightDp * metrics.density));
        int contentWidth = width;
        int contentHeight = Math.round((float) width * nativeHeight / nativeWidth);
        if (contentHeight > height) {
            contentHeight = height;
            contentWidth = Math.round((float) height * nativeWidth / nativeHeight);
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

    static final class WidgetPadding {
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
