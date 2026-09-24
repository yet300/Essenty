plugins {
    id("com.android.application")
}

android {
    namespace = "dev.essenty.androidx.packagingtest"
    compileSdk = 35

    defaultConfig {
        applicationId = "dev.essenty.androidx.packagingtest"
        minSdk = 23
        targetSdk = 35
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    implementation(project(":bridge"))
}
