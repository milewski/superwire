package com.superwire.intellij

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class SuperwireRuntimePlatformTest {
    @Test
    fun `normalizes supported operating system names`() {
        assertEquals(SuperwireOperatingSystem.Windows, SuperwireOperatingSystem.fromSystemName("Windows 11"))
        assertEquals(SuperwireOperatingSystem.MacOs, SuperwireOperatingSystem.fromSystemName("Mac OS X"))
        assertEquals(SuperwireOperatingSystem.MacOs, SuperwireOperatingSystem.fromSystemName("Darwin"))
        assertEquals(SuperwireOperatingSystem.Linux, SuperwireOperatingSystem.fromSystemName("Linux"))
        assertEquals(SuperwireOperatingSystem.Unsupported, SuperwireOperatingSystem.fromSystemName("FreeBSD"))
    }

    @Test
    fun `normalizes supported architecture names without guessing unknown targets`() {
        assertEquals(SuperwireArchitecture.X86_64, SuperwireArchitecture.fromSystemName("x86_64"))
        assertEquals(SuperwireArchitecture.X86_64, SuperwireArchitecture.fromSystemName("amd64"))
        assertEquals(SuperwireArchitecture.AArch64, SuperwireArchitecture.fromSystemName("aarch64"))
        assertEquals(SuperwireArchitecture.AArch64, SuperwireArchitecture.fromSystemName("arm64"))
        assertEquals(SuperwireArchitecture.Unsupported, SuperwireArchitecture.fromSystemName("riscv64"))
    }

    @Test
    fun `advertises only packaged resource and executable contracts`() {
        val linuxX86Platform = SuperwireRuntimePlatform(
            SuperwireOperatingSystem.Linux,
            SuperwireArchitecture.X86_64,
        )
        val linuxArmPlatform = SuperwireRuntimePlatform(
            SuperwireOperatingSystem.Linux,
            SuperwireArchitecture.AArch64,
        )
        val windowsX86Platform = SuperwireRuntimePlatform(
            SuperwireOperatingSystem.Windows,
            SuperwireArchitecture.X86_64,
        )
        val windowsArmPlatform = SuperwireRuntimePlatform(
            SuperwireOperatingSystem.Windows,
            SuperwireArchitecture.AArch64,
        )
        val macX86Platform = SuperwireRuntimePlatform(
            SuperwireOperatingSystem.MacOs,
            SuperwireArchitecture.X86_64,
        )
        val macArmPlatform = SuperwireRuntimePlatform(
            SuperwireOperatingSystem.MacOs,
            SuperwireArchitecture.AArch64,
        )
        val unsupportedPlatform = SuperwireRuntimePlatform(
            SuperwireOperatingSystem.Unsupported,
            SuperwireArchitecture.Unsupported,
        )

        assertEquals("linux-x86_64", linuxX86Platform.packagedResourceDirectory)
        assertEquals(listOf("lsp/bin/linux-x86_64/superwire-lsp"), linuxX86Platform.candidateBundledResourcePaths())
        assertEquals("windows-x86_64", windowsX86Platform.packagedResourceDirectory)
        assertEquals(listOf("lsp/bin/windows-x86_64/superwire-lsp.exe"), windowsX86Platform.candidateBundledResourcePaths())
        assertEquals("macos-x86_64", macX86Platform.packagedResourceDirectory)
        assertEquals(listOf("lsp/bin/macos-x86_64/superwire-lsp"), macX86Platform.candidateBundledResourcePaths())
        assertEquals("macos-aarch64", macArmPlatform.packagedResourceDirectory)
        assertEquals(listOf("lsp/bin/macos-aarch64/superwire-lsp"), macArmPlatform.candidateBundledResourcePaths())
        assertNull(linuxArmPlatform.packagedResourceDirectory)
        assertEquals(emptyList(), linuxArmPlatform.candidateBundledResourcePaths())
        assertNull(windowsArmPlatform.packagedResourceDirectory)
        assertEquals(emptyList(), windowsArmPlatform.candidateBundledResourcePaths())
        assertNull(unsupportedPlatform.resourceDirectory)
        assertNull(unsupportedPlatform.packagedResourceDirectory)

        assertEquals("superwire-lsp", SuperwirePluginConstants.SERVER_BINARY_NAME)
        assertEquals("textmate/", SuperwirePluginConstants.TEXTMATE_BUNDLE_DIRECTORY)
        assertEquals("superwire", SuperwirePluginConstants.LANGUAGE_ID)
        assertEquals("superwire.generated.output", SuperwirePluginConstants.GENERATED_OUTPUT_COMMAND)
    }
}
