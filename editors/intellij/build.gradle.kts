plugins {
    kotlin("jvm") version "1.9.25"
    id("org.jetbrains.intellij") version "1.17.4"
}

group = "com.engineai"
version = "0.1.0"

repositories {
    mavenCentral()
}

kotlin {
    jvmToolchain(17)
}

intellij {
    version.set("2024.1")
    type.set("IC")
    plugins.set(listOf("com.redhat.devtools.lsp4ij:0.10.0", "org.jetbrains.plugins.textmate"))
}

val buildLspBinary by tasks.registering(Exec::class) {
    workingDir = file("../..")
    commandLine("cargo", "build", "-p", "engine-ai-lsp", "--release")
}

tasks {
    patchPluginXml {
        sinceBuild.set("241")
        untilBuild.set("251.*")
    }

    processResources {
        dependsOn(buildLspBinary)

        from(file("../textmate")) {
            into("textmate")
        }

        from(file("../../target/release")) {
            include("engine-ai-lsp")
            include("engine-ai-lsp.exe")
            into("lsp/bin")
        }
    }
}
