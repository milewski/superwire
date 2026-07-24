import org.jetbrains.intellij.platform.gradle.TestFrameworkType
import java.util.zip.ZipFile

plugins {
    kotlin("jvm") version "1.9.25"
    id("org.jetbrains.intellij.platform") version "2.11.0"
}

group = "com.superwire"
version = "0.1.4"

data class LspBundleTarget(
    val rustTargetTriple: String,
    val resourceDirectory: String,
    val binaryFileName: String,
)

val lspBundleTargetsPropertyName = "superwireLspBundleTargets"
val isWindowsHost = System.getProperty("os.name").startsWith("Windows", ignoreCase = true)
val isLinuxHost = System.getProperty("os.name").startsWith("Linux", ignoreCase = true)

val allLspBundleTargets = listOf(
    LspBundleTarget("x86_64-unknown-linux-gnu", "linux-x86_64", "superwire-lsp"),
    LspBundleTarget("x86_64-pc-windows-gnu", "windows-x86_64", "superwire-lsp.exe"),
    LspBundleTarget("x86_64-apple-darwin", "macos-x86_64", "superwire-lsp"),
    LspBundleTarget("aarch64-apple-darwin", "macos-aarch64", "superwire-lsp"),
)

val defaultLspBundleTargetNames = when {
    isWindowsHost -> listOf("windows-x86_64")
    isLinuxHost -> listOf("linux-x86_64")
    else -> listOf("macos-aarch64", "macos-x86_64")
}
val requestedLspBundleTargetNames = providers.gradleProperty(lspBundleTargetsPropertyName)
    .orNull
    ?.split(',')
    ?.map(String::trim)
    ?.filter(String::isNotEmpty)
val selectedLspBundleTargetNames = requestedLspBundleTargetNames ?: defaultLspBundleTargetNames

if (selectedLspBundleTargetNames.isEmpty()) {
    throw GradleException(
        "Gradle property '$lspBundleTargetsPropertyName' must select at least one target. " +
            "Supported values: ${allLspBundleTargets.joinToString { lspBundleTarget -> lspBundleTarget.resourceDirectory }}.",
    )
}

val lspBundleTargetsToBuild = selectedLspBundleTargetNames
    .distinct()
    .map { selectedTargetName ->
        allLspBundleTargets.firstOrNull { lspBundleTarget -> lspBundleTarget.resourceDirectory == selectedTargetName }
            ?: throw GradleException(
                "Unsupported LSP bundle target '$selectedTargetName'. " +
                    "Supported values: ${allLspBundleTargets.joinToString { lspBundleTarget -> lspBundleTarget.resourceDirectory }}.",
            )
    }

val packagedLspManifestEntries = lspBundleTargetsToBuild
    .map { lspBundleTarget ->
        "lsp/bin/${lspBundleTarget.resourceDirectory}/${lspBundleTarget.binaryFileName}"
    }
    .sorted()
val generatedLspBundleManifestDirectory = layout.buildDirectory.dir("generated/lspBundleManifest")

val generateLspBundleManifest by tasks.registering {
    val manifestFile = generatedLspBundleManifestDirectory.map { generatedDirectory ->
        generatedDirectory.file("lsp/bundle-manifest.txt")
    }

    inputs.property("packagedLspManifestEntries", packagedLspManifestEntries)

    outputs.file(manifestFile)

    doLast {
        val outputFile = manifestFile.get().asFile

        outputFile.parentFile.mkdirs()
        outputFile.writeText(packagedLspManifestEntries.joinToString(separator = "\n", postfix = "\n"))
    }
}

repositories {
    mavenCentral()

    intellijPlatform {
        defaultRepositories()
    }
}

dependencies {
    testImplementation(kotlin("test-junit5"))
    testRuntimeOnly("org.junit.jupiter:junit-jupiter-engine:5.11.4")
    testRuntimeOnly("junit:junit:4.13.2")

    intellijPlatform {
        intellijIdeaCommunity("2024.3")
        bundledPlugin("org.jetbrains.plugins.textmate")
        bundledPlugin("org.intellij.plugins.markdown")
        plugin("com.redhat.devtools.lsp4ij:0.10.0")
        pluginVerifier()
        testFramework(TestFrameworkType.Platform)
    }
}

kotlin {
    jvmToolchain(21)

    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
    }
}

java {
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
}

intellijPlatform {
    buildSearchableOptions.set(false)

    pluginConfiguration {
        ideaVersion {
            sinceBuild.set("243")
            untilBuild.set("253.*")
        }
    }

    pluginVerification {
        ides {
            recommended()
        }
    }
}

val buildLspBinaries by tasks.registering {
    doLast {
        for (lspBundleTarget in lspBundleTargetsToBuild) {
            providers.exec {
                workingDir = file("../..")
                commandLine("rustup", "target", "add", lspBundleTarget.rustTargetTriple)
            }.result.get().assertNormalExitValue()

            providers.exec {
                workingDir = file("../..")
                commandLine(
                    "cargo",
                    "build",
                    "-p",
                    "superwire-lsp",
                    "--release",
                    "--target",
                    lspBundleTarget.rustTargetTriple,
                )
            }.result.get().assertNormalExitValue()
        }
    }
}

val verifyBundledLspBinary by tasks.registering {
    dependsOn(buildLspBinaries)

    doLast {
        for (lspBundleTarget in lspBundleTargetsToBuild) {
            val crossTargetBinaryPath = file("../../target/${lspBundleTarget.rustTargetTriple}/release/${lspBundleTarget.binaryFileName}")

            if (!crossTargetBinaryPath.isFile || crossTargetBinaryPath.length() == 0L || !crossTargetBinaryPath.canExecute()) {
                throw GradleException(
                    "Expected a non-empty executable bundled LSP binary at ${crossTargetBinaryPath.absolutePath}, " +
                        "but it was missing or not executable.",
                )
            }
        }
    }
}

val verifyPackagedLspBinary by tasks.registering {
    dependsOn(verifyBundledLspBinary)
    dependsOn(tasks.named("composedJar"))

    doLast {
        val finalPluginJarFile = file("build/libs/${project.name}-${project.version}.jar")

        if (!finalPluginJarFile.exists()) {
            throw GradleException("Expected final plugin JAR at ${finalPluginJarFile.absolutePath}, but it was not found.")
        }

        val expectedManifestEntries = packagedLspManifestEntries.toSet()
        val (advertisedManifestEntries, packagedBinaryEntryNames) = ZipFile(finalPluginJarFile).use { pluginJarFile ->
            val packagedEntries = pluginJarFile.entries().asSequence().toList()
            val packagedManifestEntries = packagedEntries.filter { packagedEntry ->
                packagedEntry.name == "lsp/bundle-manifest.txt"
            }

            if (packagedManifestEntries.size != 1) {
                throw GradleException(
                    "Expected exactly one lsp/bundle-manifest.txt in ${finalPluginJarFile.name}, " +
                        "found ${packagedManifestEntries.size}.",
                )
            }

            val manifestEntries = pluginJarFile
                .getInputStream(packagedManifestEntries.single())
                .bufferedReader()
                .use { manifestReader -> manifestReader.readLines() }
                .filter(String::isNotBlank)
                .toSet()
            val binaryEntryNames = packagedEntries
                .filter { packagedEntry -> !packagedEntry.isDirectory && packagedEntry.name.startsWith("lsp/bin/") }
                .map { packagedEntry -> packagedEntry.name }

            manifestEntries to binaryEntryNames
        }

        if (advertisedManifestEntries != expectedManifestEntries) {
            throw GradleException(
                "LSP manifest mismatch in ${finalPluginJarFile.name}: expected $expectedManifestEntries, " +
                    "advertised $advertisedManifestEntries.",
            )
        }

        if (
            packagedBinaryEntryNames.size != expectedManifestEntries.size ||
            packagedBinaryEntryNames.toSet() != expectedManifestEntries
        ) {
            throw GradleException(
                "Packaged LSP mismatch in ${finalPluginJarFile.name}: expected one entry for each of " +
                    "$expectedManifestEntries, found $packagedBinaryEntryNames.",
            )
        }
    }
}

tasks {
    test {
        useJUnitPlatform()
    }

    processResources {
        dependsOn(generateLspBundleManifest)

        from(generatedLspBundleManifestDirectory)
        mustRunAfter(verifyBundledLspBinary)

        from(file("icon.svg")) {
            into("icons")
        }

        from(file("icon.svg")) {
            into("META-INF")
            rename { "pluginIcon.svg" }
        }

        from(file("icon.svg")) {
            into("META-INF")
            rename { "pluginIcon_dark.svg" }
        }

        from(file("../textmate")) {
            into("textmate")
        }

        for (lspBundleTarget in lspBundleTargetsToBuild) {
            from(file("../../target/${lspBundleTarget.rustTargetTriple}/release")) {
                include(lspBundleTarget.binaryFileName)
                into("lsp/bin/${lspBundleTarget.resourceDirectory}")
            }
        }
    }

    buildPlugin {
        dependsOn(verifyPackagedLspBinary)
    }
}
