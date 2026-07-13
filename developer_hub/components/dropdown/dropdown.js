export class Dropdown {
    /**
     * @param {Object} props
     * @param {string} props.id - The ID for the dropdown container
     * @param {Array<{value: string, label: string}>|Array<string>} props.options - Dropdown options
     * @param {string} props.defaultValue - The initially selected value
     * @param {Function} props.onChange - Callback when value changes
     * @param {string} [props.className] - Extra CSS classes
     */
    constructor(props) {
        this.props = props;
        this.id = props.id || `dropdown-${Math.random().toString(36).substr(2, 9)}`;
        this.options = this._normalizeOptions(props.options || []);
        this.value = props.defaultValue || (this.options.length > 0 ? this.options[0].value : null);
        this.onChange = props.onChange || (() => {});
        
        this.element = null;
        this.selectedDisplay = null;
        this.itemsContainer = null;

        this._injectStyles();
    }

    _injectStyles() {
        const cssId = 'dropdown-component-css';
        if (!document.getElementById(cssId)) {
            const link = document.createElement('link');
            link.id = cssId;
            link.rel = 'stylesheet';
            link.href = '/components/dropdown/dropdown.css';
            document.head.appendChild(link);
        }
    }

    _normalizeOptions(options) {
        return options.map(opt => {
            if (typeof opt === 'string') return { value: opt, label: opt };
            return opt;
        });
    }

    updateOptions(newOptions, keepValue = false) {
        this.options = this._normalizeOptions(newOptions);
        if (!keepValue || !this.options.find(o => o.value === this.value)) {
            this.value = this.options.length > 0 ? this.options[0].value : null;
        }
        this.renderItems();
        this.updateDisplay();
    }

    setValue(newValue) {
        const option = this.options.find(o => o.value === newValue);
        if (option) {
            this.value = newValue;
            this.updateDisplay();
            this._highlightSelected();
        }
    }

    getValue() {
        return this.value;
    }

    updateDisplay() {
        if (!this.selectedDisplay) return;
        const selectedOpt = this.options.find(o => o.value === this.value);
        this.selectedDisplay.innerHTML = `<span class="dropdown-text">${selectedOpt ? selectedOpt.label : ''}</span>`;
        const span = this.selectedDisplay.querySelector('.dropdown-text');
        this.addHoverSlide(this.selectedDisplay, span);
        this.element.setAttribute('data-value', this.value);
    }

    _highlightSelected() {
        if (!this.itemsContainer) return;
        const items = this.itemsContainer.querySelectorAll('div[data-value]');
        items.forEach(item => {
            const check = item.querySelector('.dropdown-check');
            if (item.getAttribute('data-value') === this.value) {
                item.classList.add('same-as-selected');
                if (check) check.style.opacity = '1';
            } else {
                item.classList.remove('same-as-selected');
                if (check) check.style.opacity = '0';
            }
        });
    }

    addHoverSlide(container, textEl) {
        if (!container || !textEl) return;
        container.addEventListener('mouseenter', function() {
            if (textEl.scrollWidth > textEl.clientWidth) {
                let scrollAmount = textEl.scrollWidth - textEl.clientWidth;
                textEl.style.textOverflow = 'clip';
                textEl.style.transition = `text-indent ${scrollAmount * 20}ms linear`;
                textEl.style.textIndent = `-${scrollAmount + 4}px`;
            }
        });
        container.addEventListener('mouseleave', function() {
            textEl.style.transition = 'text-indent 0.2s ease';
            textEl.style.textIndent = '0';
            setTimeout(() => {
                if (textEl.style.textIndent === '0px' || textEl.style.textIndent === '0') {
                    textEl.style.textOverflow = 'ellipsis';
                }
            }, 200);
        });
    }

    renderItems() {
        if (!this.itemsContainer) return;
        this.itemsContainer.innerHTML = '';
        
        this.options.forEach(opt => {
            const itemDiv = document.createElement('div');
            itemDiv.setAttribute('data-value', opt.value);
            
            const isSelected = opt.value === this.value;
            itemDiv.innerHTML = `
                <span class="dropdown-text">${opt.label}</span>
                <span class="dropdown-check" style="opacity: ${isSelected ? '1' : '0'}">✓</span>
            `;
            
            const span = itemDiv.querySelector('.dropdown-text');
            this.addHoverSlide(itemDiv, span);
            
            if (isSelected) {
                itemDiv.classList.add('same-as-selected');
            }
            if (opt.disabled) {
                itemDiv.classList.add('disabled-option');
            }

            itemDiv.onclick = (e) => {
                e.stopPropagation();
                if (itemDiv.classList.contains('disabled-option')) return;
                
                this.value = opt.value;
                this.updateDisplay();
                this._highlightSelected();
                
                // Close dropdown
                this.itemsContainer.classList.add('select-hide');
                this.selectedDisplay.classList.remove('select-arrow-active');
                
                this.onChange(this.value);
            };
            
            this.itemsContainer.appendChild(itemDiv);
        });
    }

    render() {
        // Main container
        this.element = document.createElement('div');
        this.element.className = `custom-select ${this.props.className || ''}`;
        this.element.id = this.id;
        this.element.setAttribute('data-value', this.value);

        // Selected display
        this.selectedDisplay = document.createElement('div');
        this.selectedDisplay.className = 'select-selected';
        this.element.appendChild(this.selectedDisplay);

        // Options container
        this.itemsContainer = document.createElement('div');
        this.itemsContainer.className = 'select-items select-hide';
        this.element.appendChild(this.itemsContainer);

        // Populate items and display
        this.renderItems();
        this.updateDisplay();

        // Toggle logic
        this.selectedDisplay.onclick = (e) => {
            e.stopPropagation();
            const wasHidden = this.itemsContainer.classList.contains('select-hide');
            
            // Close all other selects first
            document.querySelectorAll('.select-items').forEach(el => el.classList.add('select-hide'));
            document.querySelectorAll('.select-selected').forEach(el => el.classList.remove('select-arrow-active'));
            
            if (wasHidden) {
                this.itemsContainer.classList.remove('select-hide');
                this.selectedDisplay.classList.add('select-arrow-active');
            }
        };

        // Click outside to close
        document.addEventListener('click', () => {
            this.itemsContainer.classList.add('select-hide');
            this.selectedDisplay.classList.remove('select-arrow-active');
        });

        return this.element;
    }
}
