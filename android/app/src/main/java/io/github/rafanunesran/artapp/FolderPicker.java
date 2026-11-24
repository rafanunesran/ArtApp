package io.github.rafanunesran.artapp;

import android.app.Activity;
import android.content.Intent;
import android.net.Uri;
import android.os.Bundle;

/**
 * Tela invisível que abre o seletor de pastas do Android (inclusive pastas do Google Drive) e
 * guarda a escolha em {@link Backup}. A NativeActivity não recebe onActivityResult, por isso
 * esta Activity à parte.
 */
public final class FolderPicker extends Activity {
    private static final int PICK = 1;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        if (state != null) {
            return; // recriada (giro de tela): o seletor já está aberto
        }
        Intent i = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
        i.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION
                | Intent.FLAG_GRANT_WRITE_URI_PERMISSION
                | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
                | Intent.FLAG_GRANT_PREFIX_URI_PERMISSION);
        try {
            startActivityForResult(i, PICK);
        } catch (Exception e) {
            Backup.picked("!" + e);
            finish();
        }
    }

    @Override
    protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        Uri uri = (result == RESULT_OK && data != null) ? data.getData() : null;
        if (uri == null) {
            Backup.picked("");
        } else {
            try {
                getContentResolver().takePersistableUriPermission(uri,
                        Intent.FLAG_GRANT_READ_URI_PERMISSION
                                | Intent.FLAG_GRANT_WRITE_URI_PERMISSION);
                Backup.picked(uri.toString());
            } catch (Exception e) {
                Backup.picked("!" + e);
            }
        }
        finish();
    }
}
