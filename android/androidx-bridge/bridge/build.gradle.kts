plugins {
    id("com.android.library")
}

android {
    namespace = "dev.essenty.androidx.internal"
    compileSdk = 35

    defaultConfig {
        minSdk = 23
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    api("androidx.activity:activity:1.8.2")
    api("androidx.lifecycle:lifecycle-runtime:2.7.0")
    api("androidx.lifecycle:lifecycle-viewmodel:2.7.0")
    api("androidx.savedstate:savedstate:1.2.1")
}
