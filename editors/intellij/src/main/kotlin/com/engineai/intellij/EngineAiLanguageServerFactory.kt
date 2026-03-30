package com.engineai.intellij

import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.openapi.project.Project
import com.redhat.devtools.lsp4ij.LanguageServerFactory
import com.redhat.devtools.lsp4ij.server.OSProcessStreamConnectionProvider
import com.redhat.devtools.lsp4ij.server.StreamConnectionProvider

class EngineAiLanguageServerFactory : LanguageServerFactory {
    override fun createConnectionProvider(project: Project): StreamConnectionProvider {
        val serverCommand = EngineAiServerCommandResolver.resolveServerCommand(project)

        return OSProcessStreamConnectionProvider(GeneralCommandLine(serverCommand))
    }
}
