plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

// Release signing comes from outside the repository — ~/.gradle/gradle.properties
// or the environment — and never from a committed file. Without it a release
// build is produced unsigned, ready to be signed. Every update must be signed
// with the same key, or Android refuses to install it over the old one.
fun secret(property: String, variable: String): String? =
    providers.gradleProperty(property).orElse(providers.environmentVariable(variable)).orNull

val releaseStore = secret("lclReleaseStoreFile", "LCL_RELEASE_STORE_FILE")

android {
    namespace = "io.lcl.workspace"
    compileSdk = 37

    defaultConfig {
        applicationId = "io.lcl.workspace"
        minSdk = 29
        targetSdk = 36
        // Every release raises versionCode; Android installs an update only
        // over a lower one. The properties let a build (or the update test in
        // tools/e2e.sh) set them without editing this file.
        versionCode = providers.gradleProperty("lclVersionCode").orNull?.toInt() ?: 1
        versionName = providers.gradleProperty("lclVersionName").orNull ?: "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    signingConfigs {
        if (releaseStore != null) {
            create("release") {
                storeFile = file(releaseStore)
                storePassword = secret("lclReleaseStorePassword", "LCL_RELEASE_STORE_PASSWORD")
                keyAlias = secret("lclReleaseKeyAlias", "LCL_RELEASE_KEY_ALIAS")
                keyPassword = secret("lclReleaseKeyPassword", "LCL_RELEASE_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            if (releaseStore != null) signingConfig = signingConfigs.getByName("release")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    packaging {
        resources.excludes += "/META-INF/{AL2.0,LGPL2.1}"
    }

    testOptions {
        unitTests.isReturnDefaultValues = true
    }

    lint {
        abortOnError = true
        checkDependencies = false
    }
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.foundation)
    implementation(libs.compose.material3)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.kotlinx.coroutines.android)
    implementation(libs.kotlinx.serialization.json)
    implementation(libs.zxing.core)
    implementation(libs.zxing.embedded)

    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test)

    androidTestImplementation(platform(libs.compose.bom))
    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.core)
    androidTestImplementation(libs.compose.ui.test.junit4)
    androidTestImplementation(libs.kotlinx.coroutines.test)
    debugImplementation(libs.compose.ui.test.manifest)
}
