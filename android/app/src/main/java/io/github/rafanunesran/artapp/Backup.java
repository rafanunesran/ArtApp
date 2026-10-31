package io.github.rafanunesran.artapp;

import android.content.ContentResolver;
import android.content.Context;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.provider.DocumentsContract;
import android.provider.DocumentsContract.Document;

import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;

/**
 * Backup numa pasta escolhida pelo seletor do Android (Storage Access Framework). Funciona com
 * qualquer provedor: armazenamento do aparelho, cartão SD, Google Drive... O Rust (platform.rs)
 * chama estes métodos por JNI. Os que podem falhar devolvem a mensagem de erro (null = deu certo).
 */
public final class Backup {
    /** Resultado do seletor: null = aberto, "" = cancelado, "!erro" ou a URI da pasta. */
    private static volatile String picked;

    private Backup() {}

    static void picked(String v) {
        picked = v;
    }

    /** Abre o seletor de pastas. A resposta chega em {@link #pickResult()}. */
    public static String pickFolder(Context c) {
        picked = null;
        try {
            Intent i = new Intent(c, FolderPicker.class);
            i.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            c.startActivity(i);
            return null;
        } catch (Exception e) {
            return e.toString();
        }
    }

    public static String pickResult() {
        return picked;
    }

    private static Uri children(Uri tree) {
        return DocumentsContract.buildChildDocumentsUriUsingTree(tree,
                DocumentsContract.getTreeDocumentId(tree));
    }

    /** O documento `name` dentro da pasta, ou null. */
    private static Uri find(ContentResolver r, Uri tree, String name) {
        String[] cols = {Document.COLUMN_DOCUMENT_ID, Document.COLUMN_DISPLAY_NAME};
        try (Cursor c = r.query(children(tree), cols, null, null, null)) {
            while (c != null && c.moveToNext()) {
                if (name.equals(c.getString(1))) {
                    return DocumentsContract.buildDocumentUriUsingTree(tree, c.getString(0));
                }
            }
        }
        return null;
    }

    /** Nome da pasta para mostrar nos ajustes. */
    public static String folderName(Context c, String treeUri) {
        try {
            Uri tree = Uri.parse(treeUri);
            Uri doc = DocumentsContract.buildDocumentUriUsingTree(tree,
                    DocumentsContract.getTreeDocumentId(tree));
            try (Cursor cur = c.getContentResolver().query(doc,
                    new String[] {Document.COLUMN_DISPLAY_NAME}, null, null, null)) {
                if (cur != null && cur.moveToFirst()) {
                    return cur.getString(0);
                }
            }
        } catch (Exception e) {
            // cai no texto da URI
        }
        return Uri.decode(treeUri);
    }

    private static void copy(InputStream in, OutputStream out) throws Exception {
        byte[] buf = new byte[1 << 16];
        int n;
        while ((n = in.read(buf)) > 0) {
            out.write(buf, 0, n);
        }
    }

    /**
     * Copia o arquivo local `src` para a pasta com o nome `name`, trocando o anterior. Grava
     * primeiro num arquivo ao lado e só então apaga o antigo: se faltar espaço ou a rede cair no
     * meio, o backup anterior continua lá.
     */
    public static String put(Context c, String treeUri, String src, String name) {
        try {
            ContentResolver r = c.getContentResolver();
            Uri tree = Uri.parse(treeUri);
            Uri parent = DocumentsContract.buildDocumentUriUsingTree(tree,
                    DocumentsContract.getTreeDocumentId(tree));
            String tmpName = name + ".tmp";
            Uri old = find(r, tree, tmpName);
            if (old != null) {
                DocumentsContract.deleteDocument(r, old);
            }
            Uri tmp = DocumentsContract.createDocument(r, parent, "application/octet-stream",
                    tmpName);
            if (tmp == null) {
                return "não foi possível criar o arquivo na pasta de backup";
            }
            try (InputStream in = new FileInputStream(src);
                    OutputStream out = r.openOutputStream(tmp, "w")) {
                if (out == null) {
                    return "a pasta de backup não aceitou o arquivo";
                }
                copy(in, out);
            }
            Uri prev = find(r, tree, name);
            if (prev != null) {
                DocumentsContract.deleteDocument(r, prev);
            }
            Uri renamed = null;
            try {
                renamed = DocumentsContract.renameDocument(r, tmp, name);
            } catch (Exception e) {
                renamed = null;
            }
            if (renamed == null && find(r, tree, name) == null) {
                // Provedor sem renomear: grava de novo já com o nome final.
                Uri dst = DocumentsContract.createDocument(r, parent,
                        "application/octet-stream", name);
                try (InputStream in = new FileInputStream(src);
                        OutputStream out = r.openOutputStream(dst, "w")) {
                    copy(in, out);
                }
                DocumentsContract.deleteDocument(r, tmp);
            }
            return null;
        } catch (Exception e) {
            return e.toString();
        }
    }

    /** Arquivos da pasta: uma linha por arquivo, "nome\ttamanho\tmodificado(ms)". */
    public static String list(Context c, String treeUri) {
        try {
            Uri tree = Uri.parse(treeUri);
            String[] cols = {Document.COLUMN_DISPLAY_NAME, Document.COLUMN_SIZE,
                    Document.COLUMN_LAST_MODIFIED};
            StringBuilder sb = new StringBuilder();
            try (Cursor cur = c.getContentResolver().query(children(tree), cols, null, null,
                    null)) {
                while (cur != null && cur.moveToNext()) {
                    sb.append(cur.getString(0)).append('\t').append(cur.getLong(1)).append('\t')
                            .append(cur.getLong(2)).append('\n');
                }
            }
            return sb.toString();
        } catch (Exception e) {
            return "!" + e;
        }
    }

    /** Copia `name` da pasta para o arquivo local `dest`. */
    public static String get(Context c, String treeUri, String name, String dest) {
        try {
            ContentResolver r = c.getContentResolver();
            Uri doc = find(r, Uri.parse(treeUri), name);
            if (doc == null) {
                return "arquivo não encontrado na pasta de backup";
            }
            try (InputStream in = r.openInputStream(doc);
                    OutputStream out = new FileOutputStream(dest)) {
                copy(in, out);
            }
            return null;
        } catch (Exception e) {
            return e.toString();
        }
    }
}
