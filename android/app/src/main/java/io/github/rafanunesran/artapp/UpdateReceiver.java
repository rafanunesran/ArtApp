package io.github.rafanunesran.artapp;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageInstaller;
import android.os.Build;

/** Recebe o resultado da instalação pedida por {@link Updater#install}. */
public final class UpdateReceiver extends BroadcastReceiver {
    @Override
    @SuppressWarnings("deprecation")
    public void onReceive(Context c, Intent intent) {
        int status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE);
        switch (status) {
            case PackageInstaller.STATUS_PENDING_USER_ACTION: {
                // O Android quer a confirmação da pessoa: abre o diálogo "Atualizar".
                Intent confirm = Build.VERSION.SDK_INT >= 33
                        ? intent.getParcelableExtra(Intent.EXTRA_INTENT, Intent.class)
                        : intent.getParcelableExtra(Intent.EXTRA_INTENT);
                if (confirm != null) {
                    confirm.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
                    c.startActivity(confirm);
                    Updater.installState = Updater.CONFIRMING;
                }
                break;
            }
            case PackageInstaller.STATUS_SUCCESS:
                // O sistema fecha o app para trocar a versão; normalmente nem chega aqui.
                Updater.installState = Updater.IDLE;
                break;
            case PackageInstaller.STATUS_FAILURE_ABORTED:
                Updater.installState = Updater.CANCELLED;
                break;
            default: {
                String msg = intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE);
                Updater.installMessage = msg == null ? "código " + status : msg;
                Updater.installState = Updater.FAILED;
                break;
            }
        }
    }
}
