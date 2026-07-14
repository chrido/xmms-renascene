package org.xmms.renascene;

import android.Manifest;
import android.app.NativeActivity;
import android.content.Context;
import android.content.ClipData;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.database.Cursor;
import android.media.AudioAttributes;
import android.media.AudioFocusRequest;
import android.media.AudioManager;
import android.net.Uri;
import android.os.Build;
import android.os.Handler;
import android.os.Looper;
import android.provider.DocumentsContract;
import android.provider.OpenableColumns;
import android.view.DisplayCutout;
import android.view.RoundedCorner;
import android.view.WindowInsets;

import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.ArrayList;

public final class XmmsActivity extends NativeActivity {
    private static final Handler MAIN_HANDLER = new Handler(Looper.getMainLooper());
    private static final int READ_FLAGS =
            Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION;
    static final String ACTION_PAUSE_PLAYBACK =
            "org.xmms.renascene.action.PAUSE_PLAYBACK";
    static final String ACTION_RESUME_PLAYBACK =
            "org.xmms.renascene.action.RESUME_PLAYBACK";
    static final String ACTION_NEXT_TRACK =
            "org.xmms.renascene.action.NEXT_TRACK";
    private static final int MEDIA_CONTROL_PAUSE_PLAYBACK = 1;
    private static final int MEDIA_CONTROL_RESUME_PLAYBACK = 2;
    private static final int MEDIA_CONTROL_NEXT_TRACK = 3;

    static {
        System.loadLibrary("xmms_renascene");
    }

    private native void nativeOnDocumentsSelected(
            int requestCode, String[] paths, String error);
    private native void nativeOnMediaControl(int control);

    private AudioManager audioManager;
    private AudioFocusRequest audioFocusRequest;
    private boolean mediaControlPending;
    private boolean nativeLoopReady;
    private int pendingMediaControl;
    private volatile int safeInsetLeft;
    private volatile int safeInsetTop;
    private volatile int safeInsetRight;
    private volatile int safeInsetBottom;
    private byte[] pendingDocumentContents;

    @Override
    protected void onCreate(android.os.Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        setVolumeControlStream(AudioManager.STREAM_MUSIC);
        audioManager = (AudioManager) getSystemService(Context.AUDIO_SERVICE);
        AudioAttributes attributes = new AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_MEDIA)
                .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                .build();
        audioFocusRequest = new AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
                .setAudioAttributes(attributes)
                .setOnAudioFocusChangeListener(focusChange -> {})
                .build();
        if (Build.VERSION.SDK_INT >= 33
                && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS)
                        != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(new String[] {Manifest.permission.POST_NOTIFICATIONS}, 200);
        }
        getWindow().getDecorView().setOnApplyWindowInsetsListener((view, insets) -> {
            updateSafeInsets(insets);
            return insets;
        });
        getWindow().getDecorView().requestApplyInsets();
        handleMediaControlIntent(getIntent());
    }

    @Override
    protected void onResume() {
        super.onResume();
        nativeLoopReady = true;
        getWindow().getDecorView().post(this::dispatchPendingMediaControl);
    }

    @Override
    protected void onPause() {
        nativeLoopReady = false;
        super.onPause();
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus) {
            nativeLoopReady = true;
            getWindow().getDecorView().post(this::dispatchPendingMediaControl);
        }
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        handleMediaControlIntent(intent);
    }

    @Override
    protected void onDestroy() {
        super.onDestroy();
    }

    private void dispatchMediaControl(int control) {
        nativeOnMediaControl(control);
        getWindow().getDecorView().postInvalidateOnAnimation();
    }

    private void dispatchPendingMediaControl() {
        if (!nativeLoopReady || pendingMediaControl == 0) {
            return;
        }
        int control = pendingMediaControl;
        pendingMediaControl = 0;
        dispatchMediaControl(control);
    }

    private boolean handleMediaControlIntent(Intent intent) {
        if (intent == null) {
            return false;
        }
        if (ACTION_PAUSE_PLAYBACK.equals(intent.getAction())) {
            mediaControlPending = true;
            pendingMediaControl = MEDIA_CONTROL_PAUSE_PLAYBACK;
            setIntent(new Intent(this, XmmsActivity.class));
            getWindow().getDecorView().post(this::dispatchPendingMediaControl);
            return true;
        }
        if (ACTION_RESUME_PLAYBACK.equals(intent.getAction())) {
            mediaControlPending = true;
            pendingMediaControl = MEDIA_CONTROL_RESUME_PLAYBACK;
            setIntent(new Intent(this, XmmsActivity.class));
            getWindow().getDecorView().post(this::dispatchPendingMediaControl);
            return true;
        }
        if (ACTION_NEXT_TRACK.equals(intent.getAction())) {
            mediaControlPending = true;
            pendingMediaControl = MEDIA_CONTROL_NEXT_TRACK;
            setIntent(new Intent(this, XmmsActivity.class));
            getWindow().getDecorView().post(this::dispatchPendingMediaControl);
            return true;
        }
        return false;
    }

    public void completeMediaControl() {
        runOnUiThread(() -> {
            if (mediaControlPending) {
                mediaControlPending = false;
                nativeLoopReady = false;
                moveTaskToBack(true);
            }
        });
    }

    public void updatePlaybackNotification(
            int state,
            String title,
            int bitrate,
            int frequency,
            int channels,
            long durationMs,
            long positionMs,
            long currentIndex,
            int playlistSize,
            boolean hasPrevious,
            boolean hasNext) {
        runOnUiThread(() -> {
            Intent intent = new Intent(this, XmmsPlaybackService.class)
                    .setAction(XmmsPlaybackService.ACTION_UPDATE)
                    .putExtra(XmmsPlaybackService.EXTRA_STATE, state)
                    .putExtra(XmmsPlaybackService.EXTRA_TITLE, title)
                    .putExtra(XmmsPlaybackService.EXTRA_BITRATE, bitrate)
                    .putExtra(XmmsPlaybackService.EXTRA_FREQUENCY, frequency)
                    .putExtra(XmmsPlaybackService.EXTRA_CHANNELS, channels)
                    .putExtra(XmmsPlaybackService.EXTRA_DURATION_MS, durationMs)
                    .putExtra(XmmsPlaybackService.EXTRA_POSITION_MS, positionMs)
                    .putExtra(XmmsPlaybackService.EXTRA_CURRENT_INDEX, currentIndex)
                    .putExtra(XmmsPlaybackService.EXTRA_PLAYLIST_SIZE, playlistSize)
                    .putExtra(XmmsPlaybackService.EXTRA_HAS_PREVIOUS, hasPrevious)
                    .putExtra(XmmsPlaybackService.EXTRA_HAS_NEXT, hasNext);
            if (state == 0) {
                XmmsPlayerInfoWidget.updateAll(this, title, 0, 0, 0);
                stopService(intent);
            } else if (Build.VERSION.SDK_INT >= 26) {
                startForegroundService(intent);
            } else {
                startService(intent);
            }
        });
    }

    public boolean requestPlaybackAudioFocus() {
        return audioManager != null
                && audioFocusRequest != null
                && audioManager.requestAudioFocus(audioFocusRequest)
                        == AudioManager.AUDIOFOCUS_REQUEST_GRANTED;
    }

    public void abandonPlaybackAudioFocus() {
        if (audioManager != null && audioFocusRequest != null) {
            audioManager.abandonAudioFocusRequest(audioFocusRequest);
        }
    }

    public void refreshPlayerWidgets() {
        Context applicationContext = getApplicationContext();
        MAIN_HANDLER.post(() -> {
            XmmsPlayerWidget.refreshAll(applicationContext);
            XmmsPlayerInfoWidget.refreshAll(applicationContext);
        });
    }

    public void openDocuments(int requestCode, String mimeType, boolean multiple) {
        runOnUiThread(() -> {
            Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            intent.setType(mimeType);
            intent.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, multiple);
            intent.addFlags(READ_FLAGS);
            startActivityForResult(intent, requestCode);
        });
    }

    public void openDirectory(int requestCode) {
        runOnUiThread(() -> {
            Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
            intent.addFlags(READ_FLAGS);
            startActivityForResult(intent, requestCode);
        });
    }

    public void createDocument(
            int requestCode, String mimeType, String title, byte[] contents) {
        runOnUiThread(() -> {
            pendingDocumentContents = contents;
            Intent intent = new Intent(Intent.ACTION_CREATE_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            intent.setType(mimeType);
            intent.putExtra(Intent.EXTRA_TITLE, title);
            intent.addFlags(Intent.FLAG_GRANT_WRITE_URI_PERMISSION);
            startActivityForResult(intent, requestCode);
        });
    }

    public int systemInset(int side) {
        switch (side) {
            case 0:
                return safeInsetLeft;
            case 1:
                return safeInsetTop;
            case 2:
                return safeInsetRight;
            case 3:
                return safeInsetBottom;
            default:
                return 0;
        }
    }

    private void updateSafeInsets(WindowInsets insets) {
        safeInsetLeft = calculateSafeInset(insets, 0);
        safeInsetTop = calculateSafeInset(insets, 1);
        safeInsetRight = calculateSafeInset(insets, 2);
        safeInsetBottom = calculateSafeInset(insets, 3);
    }

    private int calculateSafeInset(WindowInsets insets, int side) {
        int safeInset = 0;
        if (Build.VERSION.SDK_INT >= 30) {
            android.graphics.Insets cutout = insets.getInsetsIgnoringVisibility(
                    WindowInsets.Type.displayCutout());
            android.graphics.Insets navigation = insets.getInsetsIgnoringVisibility(
                    WindowInsets.Type.navigationBars());
            switch (side) {
                case 0:
                    safeInset = Math.max(cutout.left, navigation.left);
                    break;
                case 1:
                    safeInset = cutout.top;
                    break;
                case 2:
                    safeInset = Math.max(cutout.right, navigation.right);
                    break;
                case 3:
                    safeInset = Math.max(cutout.bottom, navigation.bottom);
                    break;
                default:
                    return 0;
            }
        } else {
            switch (side) {
                case 0:
                    safeInset = insets.getStableInsetLeft();
                    break;
                case 1:
                    break;
                case 2:
                    safeInset = insets.getStableInsetRight();
                    break;
                case 3:
                    safeInset = insets.getStableInsetBottom();
                    break;
                default:
                    return 0;
            }
            if (Build.VERSION.SDK_INT >= 28) {
                DisplayCutout cutout = insets.getDisplayCutout();
                if (cutout != null) {
                    switch (side) {
                        case 0:
                            safeInset = Math.max(safeInset, cutout.getSafeInsetLeft());
                            break;
                        case 1:
                            safeInset = Math.max(safeInset, cutout.getSafeInsetTop());
                            break;
                        case 2:
                            safeInset = Math.max(safeInset, cutout.getSafeInsetRight());
                            break;
                        case 3:
                            safeInset = Math.max(safeInset, cutout.getSafeInsetBottom());
                            break;
                        default:
                            break;
                    }
                }
            }
        }
        return Math.max(safeInset, roundedCornerInset(insets, side));
    }

    private int roundedCornerInset(WindowInsets insets, int side) {
        if (Build.VERSION.SDK_INT < 31) {
            return 0;
        }
        int width = getWindow().getDecorView().getWidth();
        int height = getWindow().getDecorView().getHeight();
        boolean landscape = width > height;
        RoundedCorner first;
        RoundedCorner second;
        switch (side) {
            case 0:
                if (!landscape) {
                    return 0;
                }
                first = insets.getRoundedCorner(RoundedCorner.POSITION_TOP_LEFT);
                second = insets.getRoundedCorner(RoundedCorner.POSITION_BOTTOM_LEFT);
                return Math.max(leftCornerInset(first), leftCornerInset(second));
            case 1:
                if (landscape) {
                    return 0;
                }
                first = insets.getRoundedCorner(RoundedCorner.POSITION_TOP_LEFT);
                second = insets.getRoundedCorner(RoundedCorner.POSITION_TOP_RIGHT);
                return Math.max(topCornerInset(first), topCornerInset(second));
            case 2:
                if (!landscape) {
                    return 0;
                }
                first = insets.getRoundedCorner(RoundedCorner.POSITION_TOP_RIGHT);
                second = insets.getRoundedCorner(RoundedCorner.POSITION_BOTTOM_RIGHT);
                return Math.max(
                        rightCornerInset(first, width),
                        rightCornerInset(second, width));
            case 3:
                if (landscape) {
                    return 0;
                }
                first = insets.getRoundedCorner(RoundedCorner.POSITION_BOTTOM_LEFT);
                second = insets.getRoundedCorner(RoundedCorner.POSITION_BOTTOM_RIGHT);
                return Math.max(
                        bottomCornerInset(first, height),
                        bottomCornerInset(second, height));
            default:
                return 0;
        }
    }

    private int leftCornerInset(RoundedCorner corner) {
        return corner == null ? 0 : corner.getCenter().x;
    }

    private int topCornerInset(RoundedCorner corner) {
        return corner == null ? 0 : corner.getCenter().y;
    }

    private int rightCornerInset(RoundedCorner corner, int width) {
        return corner == null ? 0 : Math.max(0, width - corner.getCenter().x);
    }

    private int bottomCornerInset(RoundedCorner corner, int height) {
        return corner == null ? 0 : Math.max(0, height - corner.getCenter().y);
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (resultCode != RESULT_OK || data == null) {
            if (requestCode == 105 || requestCode == 106) {
                pendingDocumentContents = null;
            }
            nativeOnDocumentsSelected(requestCode, new String[0], null);
            return;
        }
        try {
            if (requestCode == 105 || requestCode == 106) {
                Uri uri = data.getData();
                if (uri == null) {
                    throw new IllegalStateException("document provider returned no output URI");
                }
                byte[] contents = pendingDocumentContents;
                pendingDocumentContents = null;
                if (contents == null) {
                    throw new IllegalStateException("equalizer preset contents are unavailable");
                }
                try (OutputStream output = getContentResolver().openOutputStream(uri, "wt")) {
                    if (output == null) {
                        throw new IllegalStateException("could not open output document");
                    }
                    output.write(contents);
                }
                nativeOnDocumentsSelected(requestCode, new String[0], null);
                return;
            }
            if (requestCode == 104) {
                Uri treeUri = data.getData();
                if (treeUri == null) {
                    throw new IllegalStateException("document provider returned no directory");
                }
                getContentResolver().takePersistableUriPermission(
                        treeUri, Intent.FLAG_GRANT_READ_URI_PERMISSION);
                File directory = copyTreeToPrivateStorage(treeUri);
                nativeOnDocumentsSelected(
                        requestCode, new String[] {directory.getAbsolutePath()}, null);
                return;
            }
            ArrayList<Uri> uris = selectedUris(data);
            ArrayList<String> paths = new ArrayList<>(uris.size());
            for (Uri uri : uris) {
                try {
                    getContentResolver().takePersistableUriPermission(
                            uri, Intent.FLAG_GRANT_READ_URI_PERMISSION);
                } catch (SecurityException ignored) {
                    // Some providers grant only a temporary read permission.
                }
                paths.add(copyToPrivateStorage(uri).getAbsolutePath());
            }
            nativeOnDocumentsSelected(requestCode, paths.toArray(new String[0]), null);
        } catch (Exception error) {
            nativeOnDocumentsSelected(
                    requestCode, new String[0], "Failed to process selected document: " + error);
        }
    }

    private ArrayList<Uri> selectedUris(Intent data) {
        ArrayList<Uri> uris = new ArrayList<>();
        ClipData clipData = data.getClipData();
        if (clipData != null) {
            for (int index = 0; index < clipData.getItemCount(); index++) {
                uris.add(clipData.getItemAt(index).getUri());
            }
        } else if (data.getData() != null) {
            uris.add(data.getData());
        }
        return uris;
    }

    private File copyToPrivateStorage(Uri uri) throws Exception {
        File importDir = new File(getFilesDir(), "imports");
        if (!importDir.isDirectory() && !importDir.mkdirs()) {
            throw new IllegalStateException("cannot create " + importDir);
        }
        String displayName = sanitizeFileName(queryDisplayName(uri));
        File output = uniqueFile(importDir, displayName);
        try (InputStream input = getContentResolver().openInputStream(uri);
                FileOutputStream stream = new FileOutputStream(output)) {
            if (input == null) {
                throw new IllegalStateException("document provider returned no input stream");
            }
            byte[] buffer = new byte[64 * 1024];
            int count;
            while ((count = input.read(buffer)) >= 0) {
                stream.write(buffer, 0, count);
            }
        }
        return output;
    }

    private File copyTreeToPrivateStorage(Uri treeUri) throws Exception {
        File importDir = new File(getFilesDir(), "imports");
        if (!importDir.isDirectory() && !importDir.mkdirs()) {
            throw new IllegalStateException("cannot create " + importDir);
        }
        String rootId = DocumentsContract.getTreeDocumentId(treeUri);
        Uri rootUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, rootId);
        String rootName = queryDocumentName(rootUri);
        File output = uniqueDirectory(importDir, sanitizeFileName(rootName));
        if (!output.mkdirs()) {
            throw new IllegalStateException("cannot create " + output);
        }
        copyDocumentChildren(treeUri, rootId, output);
        return output;
    }

    private void copyDocumentChildren(Uri treeUri, String parentId, File outputDir)
            throws Exception {
        Uri children = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, parentId);
        String[] columns = {
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE
        };
        try (Cursor cursor = getContentResolver().query(children, columns, null, null, null)) {
            if (cursor == null) {
                throw new IllegalStateException("document provider returned no directory listing");
            }
            int idColumn = cursor.getColumnIndexOrThrow(columns[0]);
            int nameColumn = cursor.getColumnIndexOrThrow(columns[1]);
            int mimeColumn = cursor.getColumnIndexOrThrow(columns[2]);
            while (cursor.moveToNext()) {
                String documentId = cursor.getString(idColumn);
                String name = sanitizeFileName(cursor.getString(nameColumn));
                String mimeType = cursor.getString(mimeColumn);
                Uri documentUri =
                        DocumentsContract.buildDocumentUriUsingTree(treeUri, documentId);
                if (DocumentsContract.Document.MIME_TYPE_DIR.equals(mimeType)) {
                    File childDir = uniqueDirectory(outputDir, name);
                    if (!childDir.mkdirs()) {
                        throw new IllegalStateException("cannot create " + childDir);
                    }
                    copyDocumentChildren(treeUri, documentId, childDir);
                } else {
                    copyDocumentToFile(documentUri, uniqueFile(outputDir, name));
                }
            }
        }
    }

    private void copyDocumentToFile(Uri uri, File output) throws Exception {
        try (InputStream input = getContentResolver().openInputStream(uri);
                FileOutputStream stream = new FileOutputStream(output)) {
            if (input == null) {
                throw new IllegalStateException("document provider returned no input stream");
            }
            byte[] buffer = new byte[64 * 1024];
            int count;
            while ((count = input.read(buffer)) >= 0) {
                stream.write(buffer, 0, count);
            }
        }
    }

    private String queryDocumentName(Uri uri) {
        try (Cursor cursor = getContentResolver().query(
                uri,
                new String[] {DocumentsContract.Document.COLUMN_DISPLAY_NAME},
                null,
                null,
                null)) {
            if (cursor != null && cursor.moveToFirst()) {
                return cursor.getString(0);
            }
        }
        return "directory";
    }

    private String queryDisplayName(Uri uri) {
        try (Cursor cursor = getContentResolver().query(
                uri, new String[] {OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (cursor != null && cursor.moveToFirst()) {
                int column = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME);
                if (column >= 0) {
                    String name = cursor.getString(column);
                    if (name != null && !name.isEmpty()) {
                        return name;
                    }
                }
            }
        }
        String segment = uri.getLastPathSegment();
        return segment == null ? "document" : segment;
    }

    private String sanitizeFileName(String name) {
        String sanitized = name.replaceAll("[/\\\\\\n\\r\\t]", "_");
        return sanitized.isEmpty() ? "document" : sanitized;
    }

    private File uniqueFile(File directory, String name) {
        File candidate = new File(directory, name);
        if (!candidate.exists()) {
            return candidate;
        }
        int dot = name.lastIndexOf('.');
        String stem = dot > 0 ? name.substring(0, dot) : name;
        String extension = dot > 0 ? name.substring(dot) : "";
        for (int suffix = 2; ; suffix++) {
            candidate = new File(directory, stem + "-" + suffix + extension);
            if (!candidate.exists()) {
                return candidate;
            }
        }
    }

    private File uniqueDirectory(File directory, String name) {
        File candidate = new File(directory, name);
        if (!candidate.exists()) {
            return candidate;
        }
        for (int suffix = 2; ; suffix++) {
            candidate = new File(directory, name + "-" + suffix);
            if (!candidate.exists()) {
                return candidate;
            }
        }
    }
}
