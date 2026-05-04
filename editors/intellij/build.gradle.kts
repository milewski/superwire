plugins {
    kotlin("jvm") version "1.9.25"
    id("org.jetbrains.intellij") version "1.17.4"
}

group = "com.superwire"
version = "0.1.4"

data class LspBundleTarget(
    val rustTargetTriple: String,
    val resourceDirectory: String,
    val binaryFileName: String,
)

val isWindowsHost = System.getProperty("os.name").startsWith("Windows", ignoreCase = true)
val isLinuxHost = System.getProperty("os.name").startsWith("Linux", ignoreCase = true)

val allLspBundleTargets = listOf(
    LspBundleTarget("x86_64-unknown-linux-gnu", "linux-x86_64", "superwire-lsp"),
    LspBundleTarget("aarch64-unknown-linux-gnu", "linux-aarch64", "superwire-lsp"),
    LspBundleTarget("x86_64-pc-windows-gnu", "windows-x86_64", "superwire-lsp.exe"),
    LspBundleTarget("aarch64-pc-windows-gnullvm", "windows-aarch64", "superwire-lsp.exe"),
    LspBundleTarget("x86_64-apple-darwin", "macos-x86_64", "superwire-lsp"),
    LspBundleTarget("aarch64-apple-darwin", "macos-aarch64", "superwire-lsp"),
)

val lspBundleTargetsToBuild = when {
    isWindowsHost -> listOf(allLspBundleTargets.first { it.resourceDirectory == "windows-x86_64" })
    isLinuxHost -> listOf(
        allLspBundleTargets.first { it.resourceDirectory == "linux-x86_64" },
        allLspBundleTargets.first { it.resourceDirectory == "windows-x86_64" },
    )
    else -> listOf(
        allLspBundleTargets.first { it.resourceDirectory == "macos-aarch64" },
        allLspBundleTargets.first { it.resourceDirectory == "macos-x86_64" },
    )
}

repositories {
    mavenCentral()
}

kotlin {
    jvmToolchain(21)
}

intellij {
    version.set("2024.3")
    type.set("IC")
    plugins.set(listOf("com.redhat.devtools.lsp4ij:0.10.0", "org.jetbrains.plugins.textmate", "org.intellij.plugins.markdown"))
}

val buildLspBinaries by tasks.registering {
    doLast {
        for (lspBundleTarget in lspBundleTargetsToBuild) {
            project.exec {
                workingDir = file("../..")
                commandLine("rustup", "target", "add", lspBundleTarget.rustTargetTriple)
            }

            project.exec {
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
            }
        }
    }
}

val verifyBundledLspBinary by tasks.registering {
    dependsOn(buildLspBinaries)

    doLast {
        for (lspBundleTarget in lspBundleTargetsToBuild) {
            val crossTargetBinaryPath = file("../../target/${lspBundleTarget.rustTargetTriple}/release/${lspBundleTarget.binaryFileName}")

            if (!crossTargetBinaryPath.exists()) {
                throw GradleException("Expected bundled LSP binary at ${crossTargetBinaryPath.absolutePath}, but it was not found.")
            }
        }
    }
}

val verifyPackagedLspBinary by tasks.registering {
    dependsOn(tasks.named("instrumentedJar"))

    doLast {
        val instrumentedJarFile = file("build/libs/instrumented-${project.name}-${project.version}.jar")

        if (!instrumentedJarFile.exists()) {
            throw GradleException("Expected instrumented plugin JAR at ${instrumentedJarFile.absolutePath}, but it was not found.")
        }

        for (lspBundleTarget in lspBundleTargetsToBuild) {
            val packagedBinaryEntry = zipTree(instrumentedJarFile)
                .matching { include("lsp/bin/${lspBundleTarget.resourceDirectory}/${lspBundleTarget.binaryFileName}") }
                .files
                .firstOrNull()

            if (packagedBinaryEntry == null) {
                throw GradleException("LSP binary '${lspBundleTarget.binaryFileName}' was not packaged into ${instrumentedJarFile.name} under lsp/bin/${lspBundleTarget.resourceDirectory}/.")
            }
        }
    }
}

tasks {
    patchPluginXml {
        sinceBuild.set("241")
        untilBuild.set("264.*")
    }

    processResources {
        dependsOn(verifyBundledLspBinary)

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

        for (lspBundleTarget in allLspBundleTargets) {
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
