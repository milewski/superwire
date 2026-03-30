package com.engineai.intellij

import com.intellij.openapi.fileTypes.LanguageFileType
import javax.swing.Icon

object EngineAiFileType : LanguageFileType(EngineAiLanguage) {
    override fun getName(): String {
        return EngineAiPluginConstants.LANGUAGE_NAME
    }

    override fun getDescription(): String {
        return "Engine AI workflow file"
    }

    override fun getDefaultExtension(): String {
        return EngineAiPluginConstants.FILE_EXTENSION
    }

    override fun getIcon(): Icon? {
        return null
    }
}
