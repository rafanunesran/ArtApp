package io.github.rafanunesran.artapp;

import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageInfo;
import android.content.pm.PackageInstaller;
import android.net.Uri;
import android.os.Build;
import android.provider.Settings;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;

/**
 * Atualizar o app pelo próprio app: o Rust (platform.rs) chama estes métodos por JNI. A rede usa
 * o HttpURLConnection do Android (certificados e proxy do sistema) e a instalação usa o
 * PackageInstaller, que lê o APK direto da pasta do app (não precisa de FileProvider).
 *
 * Os métodos que podem falhar devolvem a mensagem de erro (ou null quando dá certo) em vez de
 * lançar exceção, para o lado nativo não ter que lidar com exceções Java.
 */
public final class Updater {
    private static final String LATEST =
            "https://api.github.com/repos/rafanunesran/ArtApp/releases/latest";

    /** Download em andamento: bytes baixados e o total (-1 = desconhecido). */
    private static volatile long downloaded;
    private static volatile long total = -1;
    private static volatile boolean cancel;

    /** Estado da instalação, escrito pelo UpdateReceiver. */
    static final int IDLE = 0, WAITING = 1, CONFIRMING = 2, FAILED = 3, CANCELLED = 4;
    static volatile int installState = IDLE;
    static volatile String installMessage = "";

    private Updater() {}

    /** O versionCode instalado (o número do build do CI). */
    @SuppressWarnings("deprecation")
    public static long installedVersion(Context c) {
        try {
            PackageInfo p = c.getPackageManager().getPackageInfo(c.getPackageName(), 0);
            return Build.VERSION.SDK_INT >= 28 ? p.getLongVersionCode() : p.versionCode;
        } catch (Exception e) {
            return 0;
        }
    }

    private static HttpURLConnection open(String url) throws Exception {
        HttpURLConnection con = (HttpURLConnection) new URL(url).openConnection();
        con.setConnectTimeout(15000);
        con.setReadTimeout(30000);
        con.setInstanceFollowRedirects(true);
        // A API do GitHub recusa pedidos sem User-Agent.
        con.setRequestProperty("User-Agent", "ArtApp-Android");
        return con;
    }

    /** O JSON da última release, ou "!" seguido do erro. */
    public static String latestRelease() {
        HttpURLConnection con = null;
        try {
            con = open(LATEST);
            con.setRequestProperty("Accept", "application/vnd.github+json");
            int code = con.getResponseCode();
            if (code != 200) {
                return "!o GitHub respondeu " + code;
            }
            ByteArrayOutputStream out = new ByteArrayOutputStream();
            try (InputStream in = con.getInputStream()) {
                byte[] buf = new byte[16384];
                for (int n; (n = in.read(buf)) > 0; ) {
                    out.write(buf, 0, n);
                }
            }
            return out.toString("UTF-8");
        } catch (Exception e) {
            return "!" + describe(e);
        } finally {
            if (con != null) {
                con.disconnect();
            }
        }
    }

    /** Baixa `url` para `dest` (por um arquivo .part, trocado no fim). */
    public static String download(String url, String dest) {
        downloaded = 0;
        total = -1;
        cancel = false;
        File part = new File(dest + ".part");
        HttpURLConnection con = null;
        try {
            con = open(url);
            int code = con.getResponseCode();
            if (code != 200) {
                return "o servidor respondeu " + code;
            }
            total = con.getContentLengthLong();
            try (InputStream in = con.getInputStream();
                 OutputStream out = new FileOutputStream(part)) {
                byte[] buf = new byte[65536];
                for (int n; (n = in.read(buf)) > 0; ) {
                    if (cancel) {
                        return "cancelado";
                    }
                    out.write(buf, 0, n);
                    downloaded += n;
                }
            }
            if (total > 0 && downloaded != total) {
                return "o download parou no meio";
            }
            File target = new File(dest);
            target.delete();
            if (!part.renameTo(target)) {
                return "não deu para gravar " + dest;
            }
            return null;
        } catch (Exception e) {
            return describe(e);
        } finally {
            if (con != null) {
                con.disconnect();
            }
            part.delete();
        }
    }

    public static long downloaded() {
        return downloaded;
    }

    public static long total() {
        return total;
    }

    public static void cancel() {
        cancel = true;
    }

    /** O ArtApp tem a permissão "Instalar apps desconhecidos"? */
    public static boolean canInstall(Context c) {
        return c.getPackageManager().canRequestPackageInstalls();
    }

    /** Abre Configurações › Instalar apps desconhecidos, já na página do ArtApp. */
    public static void openInstallSettings(Context c) {
        Intent i = new Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                Uri.parse("package:" + c.getPackageName()));
        c.startActivity(i);
    }

    /**
     * Instala o APK por cima do app. No Android 12 ou mais novo o sistema pode atualizar sem
     * perguntar (o app está atualizando a si mesmo); nos outros aparece o diálogo "Atualizar".
     * Se der certo, o Android fecha o app; ao abrir de novo já é a versão nova.
     */
    public static String install(Context c, String apk) {
        installState = WAITING;
        installMessage = "";
        try {
            File file = new File(apk);
            PackageInstaller pi = c.getPackageManager().getPackageInstaller();
            PackageInstaller.SessionParams params =
                    new PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL);
            params.setAppPackageName(c.getPackageName());
            params.setSize(file.length());
            if (Build.VERSION.SDK_INT >= 31) {
                params.setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_NOT_REQUIRED);
            }
            int id = pi.createSession(params);
            try (PackageInstaller.Session session = pi.openSession(id)) {
                try (InputStream in = new FileInputStream(file);
                     OutputStream out = session.openWrite("artapp.apk", 0, file.length())) {
                    byte[] buf = new byte[65536];
                    for (int n; (n = in.read(buf)) > 0; ) {
                        out.write(buf, 0, n);
                    }
                    session.fsync(out);
                }
                Intent done = new Intent(c, UpdateReceiver.class);
                int flags = PendingIntent.FLAG_UPDATE_CURRENT;
                if (Build.VERSION.SDK_INT >= 31) {
                    // O sistema preenche o resultado no Intent: precisa ser mutável.
                    flags |= PendingIntent.FLAG_MUTABLE;
                }
                PendingIntent pending = PendingIntent.getBroadcast(c, id, done, flags);
                session.commit(pending.getIntentSender());
            }
            return null;
        } catch (Exception e) {
            installState = FAILED;
            return describe(e);
        }
    }

    public static int installState() {
        return installState;
    }

    public static String installMessage() {
        return installMessage;
    }

    private static String describe(Exception e) {
        String m = e.getMessage();
        return m == null || m.isEmpty() ? e.getClass().getSimpleName() : m;
    }
}
