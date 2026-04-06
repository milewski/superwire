package com.superwire.intellij

import com.intellij.openapi.fileTypes.LanguageFileType
import javax.swing.Icon

object SuperwireFileType : LanguageFileType(SuperwireLanguage) {
    override fun getName(): String {
        return SuperwirePluginConstants.LANGUAGE_NAME
    }

    override fun getDescription(): String {
        return "Superwire workflow file"
    }

    override fun getDefaultExtension(): String {
        return SuperwirePluginConstants.FILE_EXTENSION
    }

    override fun getIcon(): Icon? {
        return null
    }
}
