// O app não tem código Java/Kotlin: a NativeActivity do Android carrega libartapp.so, compilada do
// Rust por `cargo ndk` (veja README.md › Android) para dentro de src/main/jniLibs.
plugins {
    id("com.android.application")
}

android {
    namespace = "io.github.rafanunesran.artapp"
    compileSdk = 35

    defaultConfig {
        applicationId = "io.github.rafanunesran.artapp"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            // Assinado com a chave de debug até existir uma chave de publicação.
            signingConfig = signingConfigs.getByName("debug")
        }
    }

    packaging {
        jniLibs {
            useLegacyPackaging = true
        }
    }
}
