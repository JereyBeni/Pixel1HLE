package com.pixel.hle;

import android.app.Activity;
import android.content.Intent;
import android.net.Uri;
import android.os.Bundle;
import android.os.Environment;
import android.provider.DocumentsContract;
import android.util.Log;
import android.view.View;
import android.widget.Button;
import android.widget.ScrollView;
import android.widget.TextView;
import android.widget.LinearLayout;
import android.graphics.Color;
import android.graphics.Typeface;
import android.view.Gravity;
import android.widget.Toast;

import java.io.BufferedReader;
import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;

/**
 * Pixel1HLE Android shell.
 *
 * Hosts the sailfish profile, APK import ("Import ya!"), live log panel,
 * and prepares the sandbox under Android/data/com.pixel.hle/.
 *
 * Full native HLE (Rust) is loaded when libpixel1_hle.so is present in jniLibs;
 * until then this activity acts as the GUI front-end + asset router.
 */
public class MainActivity extends Activity {
    private static final String TAG = "Pixel1HLE";
    private static final int REQ_IMPORT_APK = 1001;

    private TextView logView;
    private TextView statusView;
    private File sandboxRoot;
    private File appsDir;
    private File logsDir;
    private File configDir;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);

        // ── sandbox paths ──────────────────────────────────────────
        File ext = getExternalFilesDir(null);
        if (ext == null) ext = getFilesDir();
        sandboxRoot = new File(ext, "hle");
        appsDir = new File(sandboxRoot, "apps");
        logsDir = new File(sandboxRoot, "logs");
        configDir = new File(sandboxRoot, "config");
        appsDir.mkdirs();
        logsDir.mkdirs();
        configDir.mkdirs();

        copyAssetProfile();

        // ── UI ─────────────────────────────────────────────────────
        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setBackgroundColor(Color.rgb(18, 18, 24));
        root.setPadding(32, 48, 32, 32);

        TextView title = new TextView(this);
        title.setText("Pixel 1 HLE");
        title.setTextSize(28);
        title.setTextColor(Color.rgb(80, 220, 120));
        title.setTypeface(Typeface.DEFAULT_BOLD);
        title.setGravity(Gravity.CENTER_HORIZONTAL);
        root.addView(title);

        TextView sub = new TextView(this);
        sub.setText("sailfish  ·  Android 7.1  ·  com.pixel.hle");
        sub.setTextSize(13);
        sub.setTextColor(Color.rgb(140, 140, 160));
        sub.setGravity(Gravity.CENTER_HORIZONTAL);
        root.addView(sub);

        statusView = new TextView(this);
        statusView.setText("Sandbox: " + sandboxRoot.getAbsolutePath());
        statusView.setTextSize(11);
        statusView.setTextColor(Color.rgb(120, 130, 150));
        statusView.setPadding(0, 24, 0, 16);
        root.addView(statusView);

        Button importBtn = new Button(this);
        importBtn.setText("  Import ya!  ");
        importBtn.setTextSize(18);
        importBtn.setTextColor(Color.rgb(20, 20, 28));
        importBtn.setBackgroundColor(Color.rgb(80, 220, 120));
        importBtn.setOnClickListener(v -> openApkPicker());
        root.addView(importBtn);

        Button refreshBtn = new Button(this);
        refreshBtn.setText("Refresh APK list");
        refreshBtn.setOnClickListener(v -> refreshApkList());
        root.addView(refreshBtn);

        logView = new TextView(this);
        logView.setTypeface(Typeface.MONOSPACE);
        logView.setTextSize(11);
        logView.setTextColor(Color.rgb(180, 220, 180));
        logView.setPadding(8, 16, 8, 8);

        ScrollView scroll = new ScrollView(this);
        scroll.addView(logView);
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f);
        root.addView(scroll, lp);

        setContentView(root);

        log("Pixel1HLE Android shell ready");
        log("Crash / logs → " + logsDir.getAbsolutePath());
        refreshApkList();

        // Try load native HLE if present
        tryLoadNative();
    }

    private void openApkPicker() {
        Intent i = new Intent(Intent.ACTION_OPEN_DOCUMENT);
        i.addCategory(Intent.CATEGORY_OPENABLE);
        i.setType("application/vnd.android.package-archive");
        // also accept generic binary
        i.putExtra(Intent.EXTRA_MIME_TYPES, new String[]{
                "application/vnd.android.package-archive",
                "application/octet-stream",
                "*/*"
        });
        startActivityForResult(i, REQ_IMPORT_APK);
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode != REQ_IMPORT_APK || resultCode != RESULT_OK || data == null) return;
        Uri uri = data.getData();
        if (uri == null) return;

        String name = queryDisplayName(uri);
        if (name == null || !name.toLowerCase().endsWith(".apk")) {
            name = (name != null ? name : "imported") + ".apk";
        }
        File dest = new File(appsDir, name);
        try (InputStream in = getContentResolver().openInputStream(uri);
             OutputStream out = new FileOutputStream(dest)) {
            if (in == null) throw new Exception("null stream");
            byte[] buf = new byte[8192];
            int n;
            while ((n = in.read(buf)) > 0) out.write(buf, 0, n);
            log("Import ya! → " + dest.getAbsolutePath());
            Toast.makeText(this, "Imported: " + name, Toast.LENGTH_SHORT).show();
            refreshApkList();
        } catch (Exception e) {
            log("IMPORT ERROR: " + e.getMessage());
            writeCrash("import_error.txt", e);
        }
    }

    private String queryDisplayName(Uri uri) {
        try (android.database.Cursor c = getContentResolver().query(
                uri, new String[]{android.provider.OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (c != null && c.moveToFirst()) {
                return c.getString(0);
            }
        } catch (Exception ignored) {}
        return null;
    }

    private void refreshApkList() {
        File[] files = appsDir.listFiles((dir, name) -> name.toLowerCase().endsWith(".apk"));
        StringBuilder sb = new StringBuilder();
        sb.append("=== apps/ (").append(files == null ? 0 : files.length).append(") ===\n");
        if (files != null) {
            for (File f : files) {
                sb.append("  ▶ ").append(f.getName())
                  .append(String.format("  (%.1f MB)\n", f.length() / 1048576.0));
            }
        }
        log(sb.toString().trim());
    }

    private void copyAssetProfile() {
        try {
            InputStream in = getAssets().open("pixel1-sailfish.yaml");
            File out = new File(configDir, "pixel1-sailfish.yaml");
            try (OutputStream os = new FileOutputStream(out)) {
                byte[] buf = new byte[4096];
                int n;
                while ((n = in.read(buf)) > 0) os.write(buf, 0, n);
            }
            in.close();
            log("Profile copied → config/pixel1-sailfish.yaml");
        } catch (Exception e) {
            // also try reading from assets root variants
            log("Profile copy skipped: " + e.getMessage());
        }
    }

    private void tryLoadNative() {
        try {
            System.loadLibrary("pixel1_hle");
            log("Native libpixel1_hle.so loaded");
            // Future: nativeBoot(sandboxRoot.getAbsolutePath());
        } catch (UnsatisfiedLinkError e) {
            log("Native HLE not packaged yet – shell mode only");
            log("(Place libpixel1_hle.so under jniLibs/arm64-v8a to enable)");
        }
    }

    private void log(String msg) {
        Log.i(TAG, msg);
        runOnUiThread(() -> {
            logView.append(msg + "\n");
            // also append to file
            try {
                File logFile = new File(logsDir, "pixel1hle.log");
                try (FileOutputStream fos = new FileOutputStream(logFile, true)) {
                    fos.write((msg + "\n").getBytes(StandardCharsets.UTF_8));
                }
            } catch (Exception ignored) {}
        });
    }

    private void writeCrash(String name, Exception e) {
        try {
            File f = new File(logsDir, name);
            try (FileOutputStream fos = new FileOutputStream(f)) {
                fos.write(("=== CRASH ===\n" + e.toString() + "\n").getBytes(StandardCharsets.UTF_8));
                for (StackTraceElement el : e.getStackTrace()) {
                    fos.write(("  at " + el + "\n").getBytes(StandardCharsets.UTF_8));
                }
            }
        } catch (Exception ignored) {}
    }
}
