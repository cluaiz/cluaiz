/**
 * browser_voice.js
 * Zero-VRAM Audio Extension for Cluaiz Developer Hub
 * Provides host browser STT (SpeechRecognition) & TTS (SpeechSynthesis) with Live Karaoke Highlighting
 */

export class BrowserVoice {
    static recognition = null;
    static isListening = false;
    static isSpeaking = false;
    static currentUtterance = null;

    /**
     * Checks browser support for STT and TTS.
     */
    static checkSupport() {
        const hasTTS = typeof window !== 'undefined' && 'speechSynthesis' in window;
        const hasSTT = typeof window !== 'undefined' && ('SpeechRecognition' in window || 'webkitSpeechRecognition' in window);
        return { tts: hasTTS, stt: hasSTT };
    }

    /**
     * Retrieves all available browser/OS voices.
     */
    static async getVoices() {
        if (!('speechSynthesis' in window)) return [];
        let voices = window.speechSynthesis.getVoices();
        if (voices.length > 0) return voices;

        return new Promise(resolve => {
            const handler = () => {
                voices = window.speechSynthesis.getVoices();
                window.speechSynthesis.removeEventListener('voiceschanged', handler);
                resolve(voices);
            };
            window.speechSynthesis.addEventListener('voiceschanged', handler);
            setTimeout(() => {
                window.speechSynthesis.removeEventListener('voiceschanged', handler);
                resolve(window.speechSynthesis.getVoices());
            }, 600);
        });
    }

    /**
     * Starts continuous or interactive Speech-to-Text streaming.
     */
    static startSTT({ onResult, onError, onEnd, lang = 'en-US' }) {
        const SpeechRec = window.SpeechRecognition || window.webkitSpeechRecognition;
        if (!SpeechRec) {
            if (onError) onError(new Error("Browser does not support SpeechRecognition API."));
            return false;
        }

        if (this.isListening) {
            this.stopSTT();
        }

        try {
            const rec = new SpeechRec();
            rec.continuous = true;
            rec.interimResults = true;
            rec.maxAlternatives = 1;
            rec.lang = lang;

            rec.onresult = (event) => {
                let interim = '';
                let final = '';
                for (let i = event.resultIndex; i < event.results.length; ++i) {
                    const text = event.results[i][0].transcript;
                    if (event.results[i].isFinal) {
                        final += text;
                    } else {
                        interim += text;
                    }
                }
                if (onResult) {
                    onResult({ final, interim });
                }
            };

            rec.onerror = (event) => {
                if (event.error === 'no-speech') return;
                console.warn("[BrowserVoice STT] Error:", event.error);
                if (onError) onError(event);
            };

            rec.onend = () => {
                this.isListening = false;
                this.recognition = null;
                if (onEnd) onEnd();
            };

            rec.start();
            this.recognition = rec;
            this.isListening = true;
            return true;
        } catch (e) {
            console.error("[BrowserVoice STT] Failed to start:", e);
            if (onError) onError(e);
            return false;
        }
    }

    /**
     * Stops Speech-to-Text.
     */
    static stopSTT() {
        if (this.recognition && this.isListening) {
            try {
                this.recognition.stop();
            } catch (e) {}
            this.recognition = null;
            this.isListening = false;
        }
    }

    /**
     * Speaks text using host speech synthesis with live word boundary tracking.
     */
    static async speak({
        text,
        lang = 'en-US',
        rate = 1.0,
        pitch = 1.0,
        onStart,
        onBoundary,
        onEnd,
        onError
    }) {
        if (!('speechSynthesis' in window)) {
            if (onError) onError(new Error("SpeechSynthesis not available."));
            return;
        }

        this.stopTTS();

        const utterance = new SpeechSynthesisUtterance(text);
        utterance.rate = rate;
        utterance.pitch = pitch;
        utterance.lang = lang;

        const voices = await this.getVoices();
        // Priority: Natural / Neural > Exact lang match > Prefix lang match > Default
        const preferredVoice = voices.find(v => (v.name.includes("Natural") || v.name.includes("Online")) && v.lang.startsWith(lang.split('-')[0]))
            || voices.find(v => v.lang.replace('_', '-') === lang)
            || voices.find(v => v.lang.startsWith(lang.split('-')[0]))
            || voices.find(v => v.default);

        if (preferredVoice) {
            utterance.voice = preferredVoice;
        }

        utterance.onstart = () => {
            this.isSpeaking = true;
            if (onStart) onStart();
        };

        utterance.onboundary = (event) => {
            if (onBoundary && (event.name === 'word' || event.charIndex !== undefined)) {
                onBoundary(event.charIndex, event.charLength || 0);
            }
        };

        utterance.onend = () => {
            this.isSpeaking = false;
            this.currentUtterance = null;
            if (onEnd) onEnd();
        };

        utterance.onerror = (err) => {
            this.isSpeaking = false;
            this.currentUtterance = null;
            if (onError) onError(err);
        };

        this.currentUtterance = utterance;
        window.speechSynthesis.speak(utterance);
    }

    /**
     * Stops speech synthesis.
     */
    static stopTTS() {
        if (typeof window !== 'undefined' && 'speechSynthesis' in window) {
            window.speechSynthesis.cancel();
            this.isSpeaking = false;
            this.currentUtterance = null;
        }
    }

    /**
     * In-place Karaoke word highlighting helper for an HTML container element.
     */
    static highlightElementKaraoke(containerElement, fullText, charIndex, charLength = 0) {
        if (!containerElement || charIndex < 0 || charIndex >= fullText.length) return;

        let endIdx = charIndex + charLength;
        if (charLength <= 0 || endIdx > fullText.length) {
            const nextSpace = fullText.indexOf(' ', charIndex);
            endIdx = nextSpace === -1 ? fullText.length : nextSpace;
        }

        const before = fullText.substring(0, charIndex);
        const activeWord = fullText.substring(charIndex, endIdx);
        const after = fullText.substring(endIdx);

        // Escape HTML
        const escape = str => str.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

        containerElement.innerHTML = `${escape(before)}<span class="karaoke-active-word" style="background: rgba(16, 185, 129, 0.25); color: #34d399; padding: 1px 3px; border-radius: 4px; font-weight: 500; transition: background 0.15s ease;">${escape(activeWord)}</span>${escape(after)}`;
    }
}
