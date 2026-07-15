package org.xmms.renascene;

import android.app.Activity;
import android.content.ComponentName;
import android.media.browse.MediaBrowser;
import android.os.Bundle;
import android.util.Log;

import java.util.List;

public final class XmmsAutoProbeActivity extends Activity {
    private static final String TAG = "XMMS_AUTO_PROBE";
    private static final String EXTRA_WIDGET_FILENAME = "widgetFilename";
    private static final String EXTRA_WIDGET_METADATA_TITLE = "widgetMetadataTitle";
    private static final String EXTRA_PERSIST_WIDGET_STATE = "persistWidgetState";
    private static final String EXTRA_LOAD_PERSISTED_WIDGET_TITLE = "loadPersistedWidgetTitle";

    private MediaBrowser browser;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        if (getIntent().getBooleanExtra(EXTRA_LOAD_PERSISTED_WIDGET_TITLE, false)) {
            Log.i(TAG, "widget title=" + XmmsPlayerInfoWidget.persistedTitle(this));
            finish();
            return;
        }
        if (getIntent().hasExtra(EXTRA_WIDGET_FILENAME)) {
            String filename = getIntent().getStringExtra(EXTRA_WIDGET_FILENAME);
            String metadataTitle =
                    getIntent().getStringExtra(EXTRA_WIDGET_METADATA_TITLE);
            if (getIntent().getBooleanExtra(EXTRA_PERSIST_WIDGET_STATE, false)) {
                XmmsPlayerInfoWidget.updateAll(
                        this, 0, filename, metadataTitle, 0, 0, 0);
            }
            String title = XmmsPlayerInfoWidget.formatTitle(
                    this,
                    filename,
                    metadataTitle);
            Log.i(TAG, "widget title=" + title);
            finish();
            return;
        }
        browser = new MediaBrowser(
                this,
                new ComponentName(this, XmmsPlaybackService.class),
                new MediaBrowser.ConnectionCallback() {
                    @Override
                    public void onConnected() {
                        String root = browser.getRoot();
                        Log.i(TAG, "connected root=" + root);
                        browser.subscribe(root, subscriptionCallback);
                    }

                    @Override
                    public void onConnectionFailed() {
                        Log.e(TAG, "connection failed");
                        finish();
                    }
                },
                null);
        browser.connect();
    }

    @Override
    protected void onDestroy() {
        if (browser != null && browser.isConnected()) {
            browser.disconnect();
        }
        super.onDestroy();
    }

    private final MediaBrowser.SubscriptionCallback subscriptionCallback =
            new MediaBrowser.SubscriptionCallback() {
                @Override
                public void onChildrenLoaded(
                        String parentId,
                        List<MediaBrowser.MediaItem> children) {
                    Log.i(TAG, "children parent=" + parentId + " count=" + children.size());
                    if ("xmms-root".equals(parentId) && !children.isEmpty()) {
                        browser.subscribe(children.get(0).getMediaId(), this);
                        return;
                    }
                    if ("xmms-playlist".equals(parentId) && !children.isEmpty()) {
                        Log.i(
                                TAG,
                                "first title="
                                        + children.get(0).getDescription().getTitle());
                    }
                    finish();
                }

                @Override
                public void onError(String parentId) {
                    Log.e(TAG, "browse failed parent=" + parentId);
                    finish();
                }
            };
}
