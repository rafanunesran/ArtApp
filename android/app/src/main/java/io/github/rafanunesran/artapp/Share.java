package io.github.rafanunesran.artapp;

import android.content.ClipData;
import android.content.ContentProvider;
import android.content.ContentValues;
import android.content.Context;
import android.content.Intent;
import android.database.Cursor;
import android.database.MatrixCursor;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;

import java.io.File;
import java.io.FileInputStream;
import java.io.FileNotFoundException;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;

/**
 * Compartilhar uma imagem com outro app (WhatsApp, Instagram, Drive…): o Rust (platform.rs) grava
 * o arquivo e chama {@link #share} por JNI. O arquivo é copiado para a pasta cache/share e servido
 * por este provedor, só leitura e só para o app escolhido (permissão de leitura da URI), sem
 * precisar da biblioteca androidx (FileProvider).
 */
public final class Share extends ContentProvider {
    static final String AUTHORITY = "io.github.rafanunesran.artapp.share";

    private static File dir(Context c) {
        return new File(c.getCacheDir(), "share");
    }

    /** Abre um link no navegador. Devolve o erro, ou null quando deu certo. */
    public static String openUrl(Context c, String url) {
        try {
            Intent view = new Intent(Intent.ACTION_VIEW, Uri.parse(url));
            view.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            c.startActivity(view);
            return null;
        } catch (Exception e) {
            return String.valueOf(e.getMessage());
        }
    }

    /** Abre o seletor "Compartilhar" do Android. Devolve o erro, ou null quando deu certo. */
    public static String share(Context c, String path, String mime, String title) {
        try {
            File src = new File(path);
            File d = dir(c);
            if (!d.isDirectory() && !d.mkdirs()) {
                return "não foi possível criar " + d;
            }
            // Só o último compartilhado fica guardado.
            File[] old = d.listFiles();
            if (old != null) {
                for (File f : old) {
                    f.delete();
                }
            }
            File dst = new File(d, src.getName());
            try (InputStream in = new FileInputStream(src);
                 OutputStream out = new FileOutputStream(dst)) {
                byte[] buf = new byte[1 << 16];
                int n;
                while ((n = in.read(buf)) > 0) {
                    out.write(buf, 0, n);
                }
            }
            Uri uri = new Uri.Builder().scheme("content").authority(AUTHORITY)
                    .appendPath(dst.getName()).build();
            Intent send = new Intent(Intent.ACTION_SEND);
            send.setType(mime);
            send.putExtra(Intent.EXTRA_STREAM, uri);
            send.setClipData(ClipData.newRawUri(dst.getName(), uri));
            send.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
            Intent chooser = Intent.createChooser(send, title);
            chooser.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_GRANT_READ_URI_PERMISSION);
            c.startActivity(chooser);
            return null;
        } catch (Exception e) {
            return String.valueOf(e.getMessage());
        }
    }

    private File file(Uri uri) throws FileNotFoundException {
        String name = uri.getLastPathSegment();
        Context c = getContext();
        if (name == null || c == null || name.contains("/") || name.startsWith(".")) {
            throw new FileNotFoundException(String.valueOf(uri));
        }
        File f = new File(dir(c), name);
        if (!f.isFile()) {
            throw new FileNotFoundException(String.valueOf(uri));
        }
        return f;
    }

    @Override
    public boolean onCreate() {
        return true;
    }

    @Override
    public ParcelFileDescriptor openFile(Uri uri, String mode) throws FileNotFoundException {
        if (!"r".equals(mode)) {
            throw new FileNotFoundException("só leitura");
        }
        return ParcelFileDescriptor.open(file(uri), ParcelFileDescriptor.MODE_READ_ONLY);
    }

    @Override
    public String getType(Uri uri) {
        String n = String.valueOf(uri.getLastPathSegment()).toLowerCase();
        if (n.endsWith(".png")) return "image/png";
        if (n.endsWith(".jpg") || n.endsWith(".jpeg")) return "image/jpeg";
        if (n.endsWith(".gif")) return "image/gif";
        if (n.endsWith(".pdf")) return "application/pdf";
        return "application/octet-stream";
    }

    @Override
    public Cursor query(Uri uri, String[] projection, String selection, String[] args,
                        String sort) {
        File f;
        try {
            f = file(uri);
        } catch (FileNotFoundException e) {
            return null;
        }
        String[] cols = projection != null ? projection
                : new String[] {OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE};
        MatrixCursor cur = new MatrixCursor(cols, 1);
        Object[] row = new Object[cols.length];
        for (int i = 0; i < cols.length; i++) {
            if (OpenableColumns.DISPLAY_NAME.equals(cols[i])) {
                row[i] = f.getName();
            } else if (OpenableColumns.SIZE.equals(cols[i])) {
                row[i] = f.length();
            }
        }
        cur.addRow(row);
        return cur;
    }

    @Override
    public Uri insert(Uri uri, ContentValues values) {
        return null;
    }

    @Override
    public int delete(Uri uri, String selection, String[] args) {
        return 0;
    }

    @Override
    public int update(Uri uri, ContentValues values, String selection, String[] args) {
        return 0;
    }
}
