package ai.storyteller.artcraft.launcher;

import android.app.PendingIntent;
import android.app.Activity;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageInfo;
import android.content.pm.PackageInstaller;
import android.content.pm.PackageManager;
import android.graphics.Insets;
import android.net.Uri;
import android.os.Build;
import android.provider.Settings;
import android.view.WindowInsets;

import java.io.File;
import java.io.FileInputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

/**
 * The Android side of ArtCraft Launcher's package manager bridge.
 *
 * Every method takes the application context and plain strings and answers with a string:
 * "ok:<payload>" or "err:<message>" ("none" when a package isn't installed). Nothing throws across
 * the boundary and nothing calls back into Rust; installs block the calling (Rust worker) thread
 * until the system installer reports an outcome.
 */
public final class PackageHelper {
    static final String ACTION_INSTALL_RESULT = "ai.storyteller.artcraft.launcher.INSTALL_RESULT";
    /** Rust's `protocol::INSTALL_CANCELLED`: the person dismissed the system prompt. */
    static final String CANCELLED = "install cancelled";
    private static final long INSTALL_TIMEOUT_MINUTES = 20;

    static final ConcurrentHashMap<Integer, Outcome> OUTCOMES = new ConcurrentHashMap<>();

    static final class Outcome {
        final CountDownLatch done = new CountDownLatch(1);
        volatile String answer = "err:the installer never answered";
    }

    private PackageHelper() {}

    /** "ok:<versionName>" for an installed package, "none" when it isn't there. */
    public static String packageVersion(Context context, String pkg) {
        try {
            PackageInfo info = context.getPackageManager().getPackageInfo(pkg, 0);
            return "ok:" + (info.versionName == null ? "" : info.versionName);
        } catch (PackageManager.NameNotFoundException e) {
            return "none";
        } catch (RuntimeException e) {
            return "err:" + e;
        }
    }

    /** "ok:yes" or "ok:no": whether the person has allowed this app to install packages. */
    public static String canInstall(Context context) {
        try {
            return "ok:" + (context.getPackageManager().canRequestPackageInstalls() ? "yes" : "no");
        } catch (RuntimeException e) {
            return "err:" + e;
        }
    }

    /** Opens the system screen where the person allows this app to install packages. */
    public static String requestInstallPermission(Context context) {
        try {
            Intent intent = new Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:" + context.getPackageName()));
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            context.startActivity(intent);
            return "ok:";
        } catch (RuntimeException e) {
            return "err:" + e;
        }
    }

    /** Installs the APK file through a PackageInstaller session; blocks until the outcome. */
    public static String installApk(Context context, String path) {
        File file = new File(path);
        if (!file.isFile()) {
            return "err:the downloaded file is missing";
        }
        PackageInstaller installer = context.getPackageManager().getPackageInstaller();
        int sessionId = -1;
        try {
            PackageInstaller.SessionParams params = new PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL);
            sessionId = installer.createSession(params);
            Outcome outcome = new Outcome();
            OUTCOMES.put(sessionId, outcome);
            try (PackageInstaller.Session session = installer.openSession(sessionId)) {
                try (InputStream in = new FileInputStream(file); OutputStream out = session.openWrite("app.apk", 0, file.length())) {
                    byte[] buffer = new byte[64 * 1024];
                    int read;
                    while ((read = in.read(buffer)) != -1) {
                        out.write(buffer, 0, read);
                    }
                    session.fsync(out);
                }
                Intent intent = new Intent(context, InstallReceiver.class);
                intent.setAction(ACTION_INSTALL_RESULT);
                // The installer adds its status to this intent, so it has to be mutable.
                int flags = PendingIntent.FLAG_UPDATE_CURRENT | (Build.VERSION.SDK_INT >= 31 ? PendingIntent.FLAG_MUTABLE : 0);
                PendingIntent pending = PendingIntent.getBroadcast(context, sessionId, intent, flags);
                session.commit(pending.getIntentSender());
            }
            if (!outcome.done.await(INSTALL_TIMEOUT_MINUTES, TimeUnit.MINUTES)) {
                installer.abandonSession(sessionId);
                return "err:the installer didn't finish in time";
            }
            return outcome.answer;
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            return "err:interrupted";
        } catch (Exception e) {
            if (sessionId != -1) {
                try {
                    installer.abandonSession(sessionId);
                } catch (RuntimeException ignored) {
                    // the session is already gone
                }
            }
            return "err:" + e.getMessage();
        } finally {
            if (sessionId != -1) {
                OUTCOMES.remove(sessionId);
            }
        }
    }

    static final int REQ_UNINSTALL = 7;
    private static final long UNINSTALL_TIMEOUT_MINUTES = 10;
    private static volatile Outcome uninstallOutcome;
    private static volatile String uninstallPackage;

    /** Asks the system to remove a package and waits for the person's answer to its prompt. */
    public static String uninstall(final Context context, final String pkg) {
        final Activity a = MainActivity.current;
        if (a == null) {
            return "err:the window isn't ready";
        }
        if (!isInstalled(context, pkg)) {
            return "ok:";
        }
        final Outcome outcome = new Outcome();
        uninstallOutcome = outcome;
        uninstallPackage = pkg;
        a.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                try {
                    Intent intent = new Intent(Intent.ACTION_DELETE, Uri.parse("package:" + pkg));
                    intent.putExtra(Intent.EXTRA_RETURN_RESULT, true);
                    a.startActivityForResult(intent, REQ_UNINSTALL);
                } catch (RuntimeException e) {
                    outcome.answer = "err:" + e;
                    outcome.done.countDown();
                }
            }
        });
        try {
            if (!outcome.done.await(UNINSTALL_TIMEOUT_MINUTES, TimeUnit.MINUTES)) {
                return "err:the system didn't answer in time";
            }
            return outcome.answer;
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            return "err:interrupted";
        } finally {
            uninstallOutcome = null;
        }
    }

    /** The activity's answer to the uninstall prompt: what counts is whether the package is gone. */
    static void onUninstallResult(Context context) {
        Outcome outcome = uninstallOutcome;
        String pkg = uninstallPackage;
        if (outcome == null || pkg == null) {
            return;
        }
        outcome.answer = isInstalled(context, pkg) ? "err:cancelled" : "ok:";
        outcome.done.countDown();
    }

    private static boolean isInstalled(Context context, String pkg) {
        try {
            context.getPackageManager().getPackageInfo(pkg, 0);
            return true;
        } catch (PackageManager.NameNotFoundException e) {
            return false;
        }
    }

    /** "ok:top,bottom,left,right": the system bars' and cut-outs' size in pixels. */
    public static String insets(Context context) {
        Activity a = MainActivity.current;
        if (a == null) {
            return "ok:0,0,0,0";
        }
        try {
            WindowInsets wi = a.getWindow().getDecorView().getRootWindowInsets();
            if (wi == null) {
                return "ok:0,0,0,0";
            }
            if (Build.VERSION.SDK_INT >= 30) {
                Insets i = wi.getInsets(WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout());
                return "ok:" + i.top + "," + i.bottom + "," + i.left + "," + i.right;
            }
            return "ok:" + wi.getSystemWindowInsetTop() + "," + wi.getSystemWindowInsetBottom() + "," + wi.getSystemWindowInsetLeft()
                    + "," + wi.getSystemWindowInsetRight();
        } catch (RuntimeException e) {
            return "err:" + e;
        }
    }

    /** Starts a package's launch activity. */
    public static String launch(Context context, String pkg) {
        try {
            Intent intent = context.getPackageManager().getLaunchIntentForPackage(pkg);
            if (intent == null) {
                return "err:" + pkg + " has no screen to open";
            }
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            context.startActivity(intent);
            return "ok:";
        } catch (RuntimeException e) {
            return "err:" + e;
        }
    }
}
