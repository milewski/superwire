package com.superwire.intellij

import java.nio.file.Files
import java.nio.file.Path
import kotlin.io.path.name
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class WorkflowIntegrityTest {
    private val repositoryRoot = Path.of("..", "..").toAbsolutePath().normalize()
    private val workflowDirectory = repositoryRoot.resolve(".github/workflows")

    @Test
    fun `all workflows use least privilege and immutable reviewed actions`() {
        val workflowPaths = Files.list(workflowDirectory).use { workflowPathStream ->
            workflowPathStream
                .filter { workflowPath -> workflowPath.fileName.toString().endsWith(".yml") }
                .sorted()
                .toList()
        }

        assertEquals(
            setOf("ci.yml", "documentation.yml", "github-pages.yml", "intellij.yml", "laravel.yml", "superwire-executor.yml"),
            workflowPaths.map { workflowPath -> workflowPath.name }.toSet(),
        )

        val immutableActionPattern =
            Regex("""uses: [A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)+@[0-9a-f]{40} # \S+""")

        for (workflowPath in workflowPaths) {
            val workflowText = Files.readString(workflowPath).replace("\r\n", "\n")
            val actionLines = workflowText.lineSequence()
                .map(String::trim)
                .filter { workflowLine -> workflowLine.startsWith("uses:") }
                .toList()

            assertTrue(actionLines.isNotEmpty(), "${workflowPath.name} must use at least one reviewed action")
            assertTrue(
                actionLines.all(immutableActionPattern::matches),
                "${workflowPath.name} contains an unpinned or uncommented action: $actionLines",
            )
            assertTrue(
                Regex("(?m)^permissions:\n  contents: read$").containsMatchIn(workflowText),
                "${workflowPath.name} must default to contents read permission",
            )
        }
    }

    @Test
    fun `Laravel checks out its submodule before Composer`() {
        val workflowText = workflowText("laravel.yml")
        val checkoutIndex = workflowText.indexOf("uses: actions/checkout@")
        val recursiveSubmoduleIndex = workflowText.indexOf("submodules: recursive")
        val composerValidationIndex = workflowText.indexOf("composer validate --strict")

        assertTrue(workflowText.contains("- '.gitmodules'"))
        assertTrue(workflowText.contains("- 'integration/superwire-laravel'"))
        assertTrue(checkoutIndex >= 0)
        assertTrue(recursiveSubmoduleIndex > checkoutIndex)
        assertTrue(composerValidationIndex > recursiveSubmoduleIndex)
    }

    @Test
    fun `IntelliJ matrix uses managed Gradle explicit bundles and package verification`() {
        val workflowText = workflowText("intellij.yml")
        val buildConfigurationText = Files.readString(repositoryRoot.resolve("editors/intellij/build.gradle.kts"))
        val pluginBuildCommand =
            "run: gradle test verifyPlugin buildPlugin verifyPackagedLspBinary \"-PsuperwireLspBundleTargets=\${{ matrix.bundle_targets }}\""

        assertTrue(workflowText.contains("bundle_targets: linux-x86_64,windows-x86_64"))
        assertTrue(workflowText.contains("bundle_targets: macos-aarch64,macos-x86_64"))
        assertTrue(workflowText.contains("bundle_targets: windows-x86_64"))
        assertTrue(buildConfigurationText.contains("sinceBuild.set(\"243\")"))
        assertTrue(buildConfigurationText.contains("untilBuild.set(\"253.*\")"))
        assertTrue(workflowText.contains("uses: gradle/actions/setup-gradle@"))
        assertTrue(workflowText.contains("gradle-version: '8.13'"))
        assertEquals(
            2,
            workflowText.lineSequence().count { workflowLine -> workflowLine.trim() == pluginBuildCommand },
        )
        assertTrue(workflowText.contains("if: runner.os != 'Windows'"))
        assertTrue(workflowText.contains("if: runner.os == 'Windows'"))
    }

    @Test
    fun `executor pull requests never use Docker credentials`() {
        val workflowText = workflowText("superwire-executor.yml")
        val pullRequestImageStepStart = workflowText.indexOf("- name: Select pull request image repository")
        val publishImageStepStart = workflowText.indexOf("- name: Select publish image repository")
        val pullRequestImageStep = workflowText.substring(pullRequestImageStepStart, publishImageStepStart)
        val pullRequestLoginGuard = "if: github.event_name != 'pull_request'"
        val pullRequestPushGuard = "push: \${{ github.event_name != 'pull_request' }}"

        assertTrue(pullRequestImageStep.contains("if: github.event_name == 'pull_request'"))
        assertTrue(pullRequestImageStep.contains("repository=local/\${IMAGE_NAME}"))
        assertFalse(pullRequestImageStep.contains("secrets."))
        assertTrue(workflowText.contains(pullRequestLoginGuard))
        assertTrue(workflowText.contains(pullRequestPushGuard))
        assertTrue(workflowText.contains("if: github.event_name != 'pull_request'\n    runs-on: ubuntu-latest"))
    }

    private fun workflowText(fileName: String): String {
        return Files.readString(workflowDirectory.resolve(fileName)).replace("\r\n", "\n")
    }
}
