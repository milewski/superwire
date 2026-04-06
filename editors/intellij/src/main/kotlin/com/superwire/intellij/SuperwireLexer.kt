package com.superwire.intellij

import com.intellij.lexer.LexerBase
import com.intellij.psi.TokenType
import com.intellij.psi.tree.IElementType

class SuperwireLexer : LexerBase() {
    private var sourceBuffer: CharSequence = ""
    private var sourceEndOffset = 0
    private var tokenStartOffset = 0
    private var tokenEndOffset = 0
    private var currentTokenType: IElementType? = null

    override fun start(buffer: CharSequence, startOffset: Int, endOffset: Int, initialState: Int) {
        sourceBuffer = buffer
        sourceEndOffset = endOffset
        tokenStartOffset = startOffset
        tokenEndOffset = startOffset
        currentTokenType = null

        advance()
    }

    override fun getState(): Int {
        return 0
    }

    override fun getTokenType(): IElementType? {
        return currentTokenType
    }

    override fun getTokenStart(): Int {
        return tokenStartOffset
    }

    override fun getTokenEnd(): Int {
        return tokenEndOffset
    }

    override fun advance() {
        if (tokenEndOffset >= sourceEndOffset) {
            currentTokenType = null
            tokenStartOffset = sourceEndOffset
            return
        }

        tokenStartOffset = tokenEndOffset
        val firstCharacter = sourceBuffer[tokenStartOffset]

        if (firstCharacter.isWhitespace()) {
            tokenEndOffset = tokenStartOffset + 1

            while (tokenEndOffset < sourceEndOffset && sourceBuffer[tokenEndOffset].isWhitespace()) {
                tokenEndOffset += 1
            }

            currentTokenType = TokenType.WHITE_SPACE
            return
        }

        if (firstCharacter.isReferenceSeparatorCharacter()) {
            tokenEndOffset = tokenStartOffset + 1
            currentTokenType = SuperwireElementTypes.TEXT
            return
        }

        val tokenType = if (firstCharacter.isIdentifierCharacter()) {
            SuperwireElementTypes.SYMBOL
        } else {
            SuperwireElementTypes.TEXT
        }

        tokenEndOffset = tokenStartOffset + 1

        while (tokenEndOffset < sourceEndOffset) {
            val nextCharacter = sourceBuffer[tokenEndOffset]

            if (nextCharacter.isWhitespace()) {
                break
            }

            if (nextCharacter.isReferenceSeparatorCharacter()) {
                break
            }

            if (nextCharacter.isIdentifierCharacter() != firstCharacter.isIdentifierCharacter()) {
                break
            }

            tokenEndOffset += 1
        }

        currentTokenType = tokenType
    }

    override fun getBufferSequence(): CharSequence {
        return sourceBuffer
    }

    override fun getBufferEnd(): Int {
        return sourceEndOffset
    }
}

private fun Char.isIdentifierCharacter(): Boolean {
    return isLetterOrDigit() || this == '_'
}

private fun Char.isReferenceSeparatorCharacter(): Boolean {
    return this == '.' || this == '?'
}
