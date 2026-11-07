package ai.storyteller.artcraft.launcher;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageInstaller;

/** Receives the system installer's progress for a session started by {@link PackageHelper}. */
public final class InstallReceiver extends BroadcastReceiver {
    @Override
    public void onReceive(Context context, Intent intent) {
        int status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE);
        int sessionId = intent.getIntExtra(PackageInstaller.EXTRA_SESSION_ID, -1);
        String message = intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE);
        if (status == PackageInstaller.STATUS_PENDING_USER_ACTION) {
            // The system wants the person's confirmation: show its screen. The session stays open
            // and reports again once they answer.
            Intent confirm = intent.getParcelableExtra(Intent.EXTRA_INTENT);
            if (confirm != null) {
                confirm.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
                context.startActivity(confirm);
            }
            return;
        }
        PackageHelper.Outcome outcome = PackageHelper.OUTCOMES.get(sessionId);
        if (outcome == null) {
            return;
        }
        if (status == PackageInstaller.STATUS_SUCCESS) {
            outcome.answer = "ok:";
        } else if (status == PackageInstaller.STATUS_FAILURE_ABORTED) {
            outcome.answer = "err:" + PackageHelper.CANCELLED;
        } else {
            outcome.answer = "err:" + (message == null ? "the installer failed (code " + status + ")" : message);
        }
        outcome.done.countDown();
    }
}
