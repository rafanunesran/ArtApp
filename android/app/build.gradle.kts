// A NativeActivity do Android abre libartapp_boot.so (crates/artapp-boot), que carrega a
// libartapp.so do APK ou uma atualização rápida baixada. Ambas são compiladas do Rust por
// `cargo ndk` (veja README.md › Android) para dentro de src/main/jniLibs. O Java (src/main/java)
// é o que o Rust chama por JNI (atualizar, reabrir, backup).
plugins {
    id("com.android.application")
}

// Chave fixa de assinatura: o CI grava o .p12 do segredo ARTAPP_KEYSTORE_BASE64 em
// ARTAPP_KEYSTORE_FILE e passa a senha de ARTAPP_KEYSTORE_PASSWORD. Com a mesma chave em todo
// build, o Android instala a versão nova por cima da antiga sem desinstalar. Sem ela, usa a de debug.
val releaseKeystoreFile: String? = System.getenv("ARTAPP_KEYSTORE_FILE")?.takeIf { File(it).isFile }
val releaseKeystorePassword: String? = System.getenv("ARTAPP_KEYSTORE_PASSWORD")?.takeIf { it.isNotBlank() }

android {
    namespace = "io.github.rafanunesran.artapp"
    compileSdk = 35

    defaultConfig {
        applicationId = "io.github.rafanunesran.artapp"
        minSdk = 26
        targetSdk = 35
        // O CI passa o número do build, que só cresce: o Android só aceita atualizar para um
        // versionCode maior que o instalado.
        versionCode = System.getenv("ARTAPP_VERSION_CODE")?.toIntOrNull() ?: 1
        versionName = "0.1.0"
    }

    signingConfigs {
        if (releaseKeystoreFile != null && releaseKeystorePassword != null) {
            create("release") {
                storeFile = file(releaseKeystoreFile)
                storeType = "PKCS12"
                storePassword = releaseKeystorePassword
                keyAlias = "artapp"
                keyPassword = releaseKeystorePassword
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = signingConfigs.findByName("release") ?: signingConfigs.getByName("debug")
        }
    }

    packaging {
        jniLibs {
            useLegacyPackaging = true
        }
    }
}
