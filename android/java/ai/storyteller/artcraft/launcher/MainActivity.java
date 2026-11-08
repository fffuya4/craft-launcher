package ai.storyteller.artcraft.launcher;

import android.app.Activity;
import android.app.NativeActivity;
import android.content.Intent;
import android.os.Bundle;

/** The native activity, plus the one thing it must do in Java: receive the uninstall prompt's answer. */
public final class MainActivity extends NativeActivity {
    static volatile Activity current;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        current = this;
        super.onCreate(savedInstanceState);
    }

    @Override
    protected void onDestroy() {
        if (current == this) {
            current = null;
        }
        super.onDestroy();
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        if (requestCode == PackageHelper.REQ_UNINSTALL) {
            PackageHelper.onUninstallResult(getApplicationContext());
        }
        super.onActivityResult(requestCode, resultCode, data);
    }
}
