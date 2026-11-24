package io.github.rafanunesran.artapp;

import android.app.Activity;
import android.content.Intent;
import android.os.Bundle;
import android.os.Process;

/**
 * Reabre o ArtApp depois de uma atualização rápida (a libartapp.so nova só entra num processo
 * novo). O Rust chama {@link #restart} por JNI: ela abre esta Activity, que roda num processo
 * separado (":restart" no AndroidManifest), e encerra o processo do app. Esta Activity então
 * encerra o processo antigo (se ainda existir), abre o ArtApp de novo e fecha a si mesma.
 */
public final class Restart extends Activity {
    private static final String PID = "pid";

    /** Devolve null se pediu o reinício, ou a mensagem de erro. */
    public static String restart(Activity a) {
        try {
            Intent i = new Intent(a, Restart.class);
            i.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            i.putExtra(PID, Process.myPid());
            a.startActivity(i);
            Runtime.getRuntime().exit(0);
            return null;
        } catch (Exception e) {
            String m = e.getMessage();
            return m == null || m.isEmpty() ? e.getClass().getSimpleName() : m;
        }
    }

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        int pid = getIntent().getIntExtra(PID, -1);
        if (pid > 0 && pid != Process.myPid()) {
            Process.killProcess(pid);
        }
        Intent launch = getPackageManager().getLaunchIntentForPackage(getPackageName());
        if (launch != null) {
            launch.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TASK);
            startActivity(launch);
        }
        finish();
        Runtime.getRuntime().exit(0);
    }
}
