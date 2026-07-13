export class CodeEditor {
    /**
     * @param {Object} props
     * @param {string} props.id - The ID for the textarea
     * @param {string} props.mode - CodeMirror language mode
     * @param {string} [props.value=""] - Initial value
     * @param {boolean} [props.readOnly=false] - Is editor readonly
     * @param {string} [props.className] - Extra CSS classes
     * @param {Function} [props.onChange] - Callback on change
     */
    constructor(props) {
        this.props = props;
        this.id = props.id || `editor-${Math.random().toString(36).substr(2, 9)}`;
        this.mode = props.mode || "application/json";
        this.value = props.value || "";
        this.readOnly = props.readOnly || false;
        
        this.element = null;
        this.textarea = null;
        this.cm = null;
    }

    render() {
        this.element = document.createElement('div');
        this.element.className = `editor-wrapper ${this.props.className || ''}`;
        this.element.style.width = '100%';
        this.element.style.height = '100%';
        this.element.style.position = 'relative';

        this.textarea = document.createElement('textarea');
        this.textarea.id = this.id;
        this.textarea.spellcheck = false;
        this.textarea.value = this.value;
        this.element.appendChild(this.textarea);

        return this.element;
    }

    mount() {
        if (!this.textarea) return;
        
        // Ensure CodeMirror is loaded globally
        if (typeof CodeMirror === 'undefined') {
            console.error("CodeMirror is not loaded.");
            return;
        }

        this.cm = CodeMirror.fromTextArea(this.textarea, {
            mode: this.mode,
            theme: "darcula",
            lineNumbers: true,
            gutters: ["CodeMirror-lint-markers"],
            lint: true,
            indentUnit: 2,
            matchBrackets: true,
            autoCloseBrackets: true,
            readOnly: this.readOnly ? "nocursor" : false
        });
        
        this.cm.setSize("100%", "100%");

        if (this.props.onChange) {
            this.cm.on("change", () => {
                this.value = this.cm.getValue();
                this.props.onChange(this.value);
            });
        }
    }

    setValue(newValue) {
        this.value = newValue;
        if (this.cm) {
            this.cm.setValue(newValue);
        } else if (this.textarea) {
            this.textarea.value = newValue;
        }
    }

    getValue() {
        return this.cm ? this.cm.getValue() : this.value;
    }

    setOption(option, value) {
        if (this.cm) {
            this.cm.setOption(option, value);
        }
    }

    refresh() {
        if (this.cm) {
            this.cm.refresh();
        }
    }
}
