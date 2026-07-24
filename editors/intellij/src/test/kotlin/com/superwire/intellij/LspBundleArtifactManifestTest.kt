package com.superwire.intellij

import java.nio.file.Files
import java.nio.file.Path
import kotlin.test.Test
import kotlin.test.assertEquals

class LspBundleArtifactManifestTest {
    private val repositoryRoot = Path.of("..", "..").toAbsolutePath().normalize()

    @Test
    fun `runtime build and CI advertise the same packaged platform matrix`() {
        val runtimePlatforms = listOf(
            SuperwireRuntimePlatform(SuperwireOperatingSystem.Linux, SuperwireArchitecture.X86_64),
            SuperwireRuntimePlatform(SuperwireOperatingSystem.Linux, SuperwireArchitecture.AArch64),
            SuperwireRuntimePlatform(SuperwireOperatingSystem.Windows, SuperwireArchitecture.X86_64),
            SuperwireRuntimePlatform(SuperwireOperatingSystem.Windows, SuperwireArchitecture.AArch64),
            SuperwireRuntimePlatform(SuperwireOperatingSystem.MacOs, SuperwireArchitecture.X86_64),
            SuperwireRuntimePlatform(SuperwireOperatingSystem.MacOs, SuperwireArchitecture.AArch64),
        )
        val runtimeManifestEntries = runtimePlatforms
            .flatMap(SuperwireRuntimePlatform::candidateBundledResourcePaths)
            .toSet()
        val expectedManifestEntries = setOf(
            "lsp/bin/linux-x86_64/superwire-lsp",
            "lsp/bin/windows-x86_64/superwire-lsp.exe",
            "lsp/bin/macos-x86_64/superwire-lsp",
            "lsp/bin/macos-aarch64/superwire-lsp",
        )

        val buildScriptText = Files.readString(repositoryRoot.resolve("editors/intellij/build.gradle.kts"))
        val configuredManifestEntriesByDirectory = Regex(
            """LspBundleTarget\("[^"]+", "([^"]+)", "([^"]+)"\)""",
        ).findAll(buildScriptText).associate { targetMatch ->
            val resourceDirectory = targetMatch.groupValues[1]
            val binaryFileName = targetMatch.groupValues[2]

            resourceDirectory to "lsp/bin/$resourceDirectory/$binaryFileName"
        }
        val configuredManifestEntries = configuredManifestEntriesByDirectory.values.toSet()

        val workflowText = Files.readString(repositoryRoot.resolve(".github/workflows/intellij.yml"))
        val workflowBundleDirectories = Regex("""bundle_targets:\s*([^\r\n]+)""")
            .findAll(workflowText)
            .flatMap { bundleTargetMatch -> bundleTargetMatch.groupValues[1].split(',').asSequence() }
            .map(String::trim)
            .toSet()
        val workflowManifestEntries = workflowBundleDirectories
            .map { resourceDirectory -> configuredManifestEntriesByDirectory.getValue(resourceDirectory) }
            .toSet()

        assertEquals(expectedManifestEntries, runtimeManifestEntries)
        assertEquals(expectedManifestEntries, configuredManifestEntries)
        assertEquals(expectedManifestEntries, workflowManifestEntries)
    }
}
