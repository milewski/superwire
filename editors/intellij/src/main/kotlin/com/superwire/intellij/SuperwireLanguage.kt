package com.superwire.intellij

import com.intellij.lang.InjectableLanguage
import com.intellij.lang.Language

object SuperwireLanguage : Language(SuperwirePluginConstants.LANGUAGE_ID), InjectableLanguage {
    override fun getDisplayName(): String {
        return SuperwirePluginConstants.LANGUAGE_NAME
    }
}
