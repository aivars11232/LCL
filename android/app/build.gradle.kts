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

// The update signing keys the app trusts: update/trusted_keys.txt, the same
// list the PC updater is built with. Only public keys are there.
fun keyLines(text: String) = text.lines().map { it.trim() }.filter { it.isNotEmpty() && !it.startsWith("#") }.joinToString("\\n")
val trustedUpdateKeys = keyLines(rootProject.file("../update/trusted_keys.txt").readText())
// A debug build may name a local test release server and test keys, for the
// end-to-end update test; a release build never has either.
val updateTestEndpoint = providers.gradleProperty("lclUpdateTestEndpoint").orNull ?: ""
val updateTestKeys = providers.gradleProperty("lclUpdateTestKeys").orNull?.let { keyLines(file(it).readText()) } ?: ""

val manualAssets: File = layout.buildDirectory.dir("generated/manual-assets").get().asFile
val syncManual by tasks.registering(Sync::class) {
    from(rootProject.file("../users_manual")) { include("*.md", "MANIFEST.json") }
    from(rootProject.file("../impl/crates/lcl-workspace/assets/manual")) { include("manual.html", "manual.js", "manual.css") }
    into(manualAssets.resolve("manual"))
}

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
        buildConfigField("String", "UPDATE_TRUSTED_KEYS", "\"$trustedUpdateKeys\"")
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
        debug {
            buildConfigField("String", "UPDATE_TEST_ENDPOINT", "\"$updateTestEndpoint\"")
            buildConfigField("String", "UPDATE_TEST_KEYS", "\"$updateTestKeys\"")
        }
        release {
            buildConfigField("String", "UPDATE_TEST_ENDPOINT", "\"\"")
            buildConfigField("String", "UPDATE_TEST_KEYS", "\"\"")
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

    // The Users Manual snapshot and the viewer the desktop workspace serves,
    // copied from the repository at build time: one source for both.
    sourceSets.getByName("main").assets.srcDir(manualAssets)
}

tasks.named("preBuild") { dependsOn(syncManual) }

tasks.withType<Test>().configureEach {
    dependsOn(syncManual)
    systemProperty("lcl.manualAssets", manualAssets.resolve("manual").path)
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
