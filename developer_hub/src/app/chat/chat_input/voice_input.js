// ─── Voice Input Component (Pure Bulk Audio Recording & Engine STT Dispatcher) ───

let activeAudioModelId = null;

export function checkAudioModelAndToggleMic(installedModels) {
    const installed = Array.isArray(installedModels) ? installedModels : Object.values(installedModels || {});

    // Clean API schema check using model registry capabilities & supported_tasks
    const audioModel = installed.find(m => {
        const isAudioCategory = m.category === 'audio';
        const tasks = m.capabilities?.supported_tasks || m.capabilities?.explicit_tasks || m.supported_tasks || [];
        return isAudioCategory || tasks.includes('speech_to_text');
    });

    if (audioModel) {
        activeAudioModelId = audioModel.id;
    }

    const micWrapper = document.getElementById('mic-wrapper');
    if (micWrapper) {
        micWrapper.style.display = audioModel ? 'flex' : 'none';
    }
}

export function setupMicVoiceInput(textarea) {
    const btnMic = document.getElementById('btn-mic');
    if (!btnMic) return;

    let isRecording = false;
    let isTranscribing = false;
    let mediaRecorder = null;
    let audioChunks = [];
    let audioCtx = null;
    let animFrameId = null;
    let micStream = null;
    let sttDotTimer = null;

    function stopDotTimer() {
        if (sttDotTimer) {
            clearInterval(sttDotTimer);
            sttDotTimer = null;
        }
    }

    function startDotTimer(priorText) {
        stopDotTimer();
        let f = 0;
        const frames = ['.', '..', '...'];
        if (textarea) {
            textarea.value = (priorText ? priorText + ' ' : '') + frames[0];
            textarea.placeholder = frames[0];
            textarea.dispatchEvent(new Event('input'));
        }
        sttDotTimer = setInterval(() => {
            f = (f + 1) % 3;
            if (textarea) {
                textarea.value = (priorText ? priorText + ' ' : '') + frames[f];
                textarea.placeholder = frames[f];
                textarea.dispatchEvent(new Event('input'));
            }
        }, 250);
    }

    const defaultMicSvg = `<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-5 h-5"><path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3z"></path><path d="M19 10v2a7 7 0 0 1-14 0v-2"></path><line x1="12" y1="19" x2="12" y2="22"></line></svg>`;
    const spinnerSvg = `<svg class="animate-spin w-4 h-4 text-accent" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" style="animation: spin 1s linear infinite;"><circle style="opacity: 0.25;" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle><path style="opacity: 0.75;" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"></path></svg>`;

    if (btnMic) {
        btnMic.innerHTML = defaultMicSvg;
    }

    // ─── Audio Frequency Visualizer (Color-Shifting Bouncing Glow Ring) ────
    function startAudioVisualizer(stream) {
        try {
            const AudioContext = window.AudioContext || window.webkitAudioContext;
            if (!AudioContext) return;

            audioCtx = new AudioContext();
            const analyser = audioCtx.createAnalyser();
            analyser.fftSize = 64;
            analyser.smoothingTimeConstant = 0.55; // Smooth balanced decay

            const source = audioCtx.createMediaStreamSource(stream);
            source.connect(analyser);

            const dataArray = new Uint8Array(analyser.frequencyBinCount);

            function updateGlow() {
                analyser.getByteFrequencyData(dataArray);
                let sum = 0;
                for (let i = 0; i < dataArray.length; i++) {
                    sum += dataArray[i];
                }
                const average = sum / dataArray.length;

                // Subtle smooth sensitivity curve
                const rawNorm = Math.min(1, Math.max(0, (average - 12) / 70));
                const norm = Math.pow(rawNorm, 1.8);

                // Pure Red (239, 68, 68) to Deep Crimson Dark Red (136, 19, 55)
                const r = Math.round(239 - norm * 103); // 239 -> 136
                const g = Math.round(68 - norm * 49);   // 68 -> 19
                const b = Math.round(68 - norm * 30);   // 68 -> 38

                const scale = 1 + norm * 0.07;
                const glowSpread = Math.round(3 + norm * 8);
                const shadowOpacity = (0.3 + norm * 0.35).toFixed(2);

                if (btnMic && isRecording) {
                    btnMic.style.color = `rgb(${r}, ${g}, ${b})`;
                    btnMic.style.transform = `scale(${scale})`;
                    btnMic.style.boxShadow = `0 0 ${glowSpread}px rgba(${r}, ${g}, ${b}, ${shadowOpacity})`;
                    btnMic.style.backgroundColor = `rgba(${r}, ${g}, ${b}, 0.2)`;

                    // Pulse mic icon SVG inside per word
                    const micSvg = btnMic.querySelector('svg');
                    if (micSvg) {
                        micSvg.style.transform = `scale(${1 + norm * 0.1})`;
                        micSvg.style.transition = 'transform 0.04s ease-out';
                    }

                    // Outer border/shadow AND inside inset shadow both pulse dynamically with sound volume
                    const inputContainer = document.getElementById('chat-input-container');
                    if (inputContainer) {
                        const borderAlpha = (0.2 + norm * 0.75).toFixed(2);
                        const spread = Math.round(4 + norm * 18);
                        const shadowAlpha = (0.15 + norm * 0.6).toFixed(2);

                        const insetSpread = Math.round(8 + norm * 22);
                        const insetAlpha = (0.35 + norm * 0.45).toFixed(2);

                        inputContainer.style.setProperty('transition', 'none', 'important');
                        inputContainer.style.setProperty('border-color', `rgba(${r}, ${g}, ${b}, ${borderAlpha})`, 'important');
                        inputContainer.style.setProperty('box-shadow', `0 0 ${spread}px rgba(${r}, ${g}, ${b}, ${shadowAlpha}), inset 0 0 ${insetSpread}px rgba(0, 0, 0, ${insetAlpha})`, 'important');
                    }
                }

                animFrameId = requestAnimationFrame(updateGlow);
            }
            updateGlow();
        } catch (e) {
            console.warn('Audio visualizer init error:', e);
        }
    }

    function stopAudioVisualizer() {
        if (animFrameId) {
            cancelAnimationFrame(animFrameId);
            animFrameId = null;
        }
        if (audioCtx) {
            try {
                audioCtx.close();
            } catch (e) { }
            audioCtx = null;
        }
    }

    function resetMicUi() {
        stopAudioVisualizer();
        stopDotTimer();
        isRecording = false;
        isTranscribing = false;

        const inputContainer = document.getElementById('chat-input-container');
        if (inputContainer) {
            inputContainer.style.removeProperty('transition');
            inputContainer.style.removeProperty('border-color');
            inputContainer.style.removeProperty('box-shadow');
            inputContainer.style.borderColor = '';
            inputContainer.style.boxShadow = '';
        }

        if (btnMic) {
            btnMic.disabled = false;
            btnMic.style.pointerEvents = 'auto';
            btnMic.style.cursor = 'pointer';
            btnMic.style.color = '#9ca3af';
            btnMic.style.backgroundColor = 'transparent';
            btnMic.style.boxShadow = 'none';
            btnMic.style.borderColor = 'transparent';
            btnMic.style.transform = 'scale(1)';
            btnMic.innerHTML = defaultMicSvg;
            btnMic.setAttribute('title', 'Record speech via microphone');
        }
    }

    // ─── Mic Click Handler ───────────────────────────────────────────
    btnMic.addEventListener('click', async (e) => {
        if (e) {
            e.preventDefault();
            e.stopPropagation();
        }

        if (isTranscribing) {
            return; // Ignore clicks during transcription loading state
        }

        // Toggle Stop Recording if active
        if (isRecording) {
            isRecording = false;
            isTranscribing = true;
            stopAudioVisualizer();

            const inputContainer = document.getElementById('chat-input-container');
            if (inputContainer) {
                inputContainer.style.removeProperty('transition');
                inputContainer.style.removeProperty('border-color');
                inputContainer.style.removeProperty('box-shadow');
                inputContainer.style.borderColor = '';
                inputContainer.style.boxShadow = '';
            }

            // IMMEDIATELY show Blue Spinning Loader & Disable Mic Button Clicks
            btnMic.disabled = true;
            btnMic.style.pointerEvents = 'none';
            btnMic.style.cursor = 'not-allowed';
            btnMic.style.color = '#3b82f6';
            btnMic.style.backgroundColor = 'rgba(59, 130, 246, 0.2)';
            btnMic.style.boxShadow = '0 0 14px rgba(59, 130, 246, 0.6)';
            btnMic.style.transform = 'scale(1)';
            btnMic.innerHTML = spinnerSvg;
            btnMic.setAttribute('title', 'Transcribing full audio via Engine API...');

            const priorText = textarea ? textarea.value.replace(/(\.|\.\.|\.\.\.)$/g, '').trim() : '';

            // 500ms Smooth Delay before starting typing dots loader
            setTimeout(() => {
                if (isTranscribing) {
                    startDotTimer(priorText);
                }
            }, 500);

            if (mediaRecorder && mediaRecorder.state !== 'inactive') {
                try {
                    mediaRecorder.requestData();
                    mediaRecorder.stop();
                } catch (err) {
                    console.warn('MediaRecorder stop warning:', err);
                }
            }
            return;
        }

        // Start Recording Bulk Audio
        try {
            stopDotTimer(); // Ensure zero dots during live recording
            micStream = await navigator.mediaDevices.getUserMedia({ audio: true });
            audioChunks = [];
            isRecording = true;

            btnMic.style.color = '#ef4444';
            btnMic.style.backgroundColor = 'rgba(239, 68, 68, 0.2)';
            btnMic.setAttribute('title', 'Recording full audio... Click again to stop & send');

            startAudioVisualizer(micStream);

            // MediaRecorder for Engine API dispatch
            let options = {};
            if (MediaRecorder.isTypeSupported('audio/mp4')) {
                options = { mimeType: 'audio/mp4' };
            } else if (MediaRecorder.isTypeSupported('audio/aac')) {
                options = { mimeType: 'audio/aac' };
            } else if (MediaRecorder.isTypeSupported('audio/webm')) {
                options = { mimeType: 'audio/webm' };
            }

            mediaRecorder = new MediaRecorder(micStream, options);

            mediaRecorder.ondataavailable = (event) => {
                if (event.data && event.data.size > 0) {
                    audioChunks.push(event.data);
                }
            };

            mediaRecorder.onstop = async () => {
                const originalPlaceholder = textarea ? textarea.placeholder : 'Ask AI...';
                const priorText = textarea ? textarea.value.replace(/(\.|\.\.|\.\.\.)$/g, '').trim() : '';

                // Stop microphone stream tracks
                if (micStream) {
                    micStream.getTracks().forEach(track => track.stop());
                    micStream = null;
                }

                const mimeType = mediaRecorder.mimeType || 'audio/webm';
                const audioBlob = new Blob(audioChunks, { type: mimeType });
                const reader = new FileReader();
                reader.readAsDataURL(audioBlob);

                reader.onloadend = async () => {
                    const base64Data = reader.result;
                    if (!base64Data || audioBlob.size < 500) {
                        console.warn('Audio recording blob is empty or too short');
                        resetMicUi();
                        return;
                    }
                    try {
                        const reqHeaders = { 'Content-Type': 'application/json' };
                        const apiToken = localStorage.getItem('cluaiz_api_token');
                        if (apiToken) {
                            reqHeaders['Authorization'] = 'Bearer ' + apiToken;
                        }

                        const res = await fetch(window.getApiBaseUrl() + '/v1/audio/execute', {
                            method: 'POST',
                            headers: reqHeaders,
                            body: JSON.stringify({
                                model: 'auto',
                                task: 'speech_to_text',
                                keep_alive: -1,
                                stream: true,
                                instruction: 'Transcribe the audio accurately in the exact language spoken by the user.',
                                input_source: {
                                    type: 'base64',
                                    data: base64Data
                                },
                                parameters: {
                                    language: ''
                                }
                            })
                        });

                        stopDotTimer();

                        const contentType = res.headers.get('content-type') || '';
                        if (res.ok && contentType.includes('text/event-stream') && res.body) {
                            const reader = res.body.getReader();
                            const decoder = new TextDecoder('utf-8');
                            let currentText = priorText ? priorText + ' ' : '';
                            let buffer = '';
                            if (textarea) textarea.value = currentText;

                            while (true) {
                                const { done, value } = await reader.read();
                                if (done) break;
                                buffer += decoder.decode(value, { stream: true });
                                const lines = buffer.split('\n');
                                buffer = lines.pop() || '';
                                for (const line of lines) {
                                    const trimmed = line.trim();
                                    if (trimmed.startsWith('data:')) {
                                        const jsonStr = trimmed.slice(5).trim();
                                        try {
                                            const payload = JSON.parse(jsonStr);
                                            if (payload.token && textarea) {
                                                currentText += payload.token;
                                                textarea.value = currentText;
                                                textarea.dispatchEvent(new Event('input'));
                                                textarea.scrollTop = textarea.scrollHeight;
                                            }
                                        } catch (e) {
                                            // Non-JSON SSE line
                                        }
                                    }
                                }
                            }
                        } else if (res.ok) {
                            const out = await res.json();
                            const transcriptText = (typeof out.output === 'string' ? out.output : (out.output?.text || out.text || '')).trim();
                            if (textarea && transcriptText) {
                                textarea.value = (priorText ? priorText + ' ' : '') + transcriptText;
                                textarea.dispatchEvent(new Event('input'));
                                textarea.scrollTop = textarea.scrollHeight;
                            }
                        } else {
                            const errJson = await res.json().catch(() => ({}));
                            console.error('STT API execution error status:', res.status, errJson);
                            if (textarea) textarea.value = priorText;
                        }
                    } catch (err) {
                        console.error('Audio STT execution error:', err);
                        if (textarea) textarea.value = priorText;
                    } finally {
                        stopDotTimer();
                        if (textarea) {
                            textarea.placeholder = originalPlaceholder;
                            textarea.dispatchEvent(new Event('input'));
                        }
                        resetMicUi();
                    }
                };
            };

            mediaRecorder.start(250);

        } catch (err) {
            console.error('Microphone access error:', err);
            isRecording = false;
            resetMicUi();
        }
    });
}
