;;; slant-mode.el --- Major mode for the Slant language -*- lexical-binding: t; -*-

;; Version: 0.1.0
;; Package-Requires: ((emacs "27.1"))
;; Keywords: languages
;; SPDX-License-Identifier: MIT OR Apache-2.0

;;; Commentary:

;; A major mode for Slant (`.sl') source: highlighting, comment and string
;; syntax, indentation, imenu, and formatting through `slc fmt'.
;;
;;   (add-to-list 'load-path "/path/to/slc/editors/emacs")
;;   (require 'slant-mode)
;;
;; Indentation follows the layout `slc fmt' writes — a level per bracket,
;; and a chain's `|' one level in from where the chain began — so typing
;; and formatting agree.  `slant-format-buffer' (C-c C-f) runs the
;; formatter itself; set `slant-format-on-save' to run it on every save.

;;; Code:

(defgroup slant nil
  "Support for the Slant language."
  :group 'languages
  :prefix "slant-")

(defcustom slant-indent-offset 4
  "Columns per level of indentation, as `slc fmt' writes them."
  :type 'integer
  :safe #'integerp)

(defcustom slant-slc-command "slc"
  "The `slc' executable, for `slant-format-buffer'."
  :type 'string)

(defcustom slant-format-on-save nil
  "When non-nil, format a Slant buffer with `slc fmt' before saving it."
  :type 'boolean
  :safe #'booleanp)

;;; Syntax

(defvar slant-mode-syntax-table
  (let ((table (make-syntax-table)))
    ;; `//' to the end of the line, and `/* … */', which nests.
    (modify-syntax-entry ?/ ". 124b" table)
    (modify-syntax-entry ?* ". 23n" table)
    (modify-syntax-entry ?\n "> b" table)
    (modify-syntax-entry ?\" "\"" table)
    (modify-syntax-entry ?\\ "\\" table)
    (modify-syntax-entry ?_ "_" table)
    ;; A `'' is a quote only around a char literal, which
    ;; `slant--syntax-propertize' finds.
    (modify-syntax-entry ?' "." table)
    ;; `<' and `>' close a chain or bracket type arguments; neither use
    ;; pairs reliably — `<v | f' has no `>' — so they are punctuation.
    (dolist (char '(?< ?> ?| ?& ?+ ?- ?= ?% ?! ?@ ?: ?. ?, ?\;))
      (modify-syntax-entry char "." table))
    table)
  "Syntax table for `slant-mode'.")

(defconst slant--syntax-propertize
  (syntax-propertize-rules
   ;; 'a', '\n', '\'' and '"': the quotes delimit, so a `"' between them
   ;; opens no string.
   ("\\('\\)\\(?:\\\\.\\|[^\\\\'\n]\\)\\('\\)"
    (1 "\"") (2 "\"")))
  "Mark the quotes of char literals as string delimiters.")

;;; Highlighting

(defconst slant-keywords
  '("fn" "mu" "command" "mod" "use" "trait" "impl" "for" "effect" "handle"
    "handler" "with" "reset" "match" "select" "let" "data" "enum" "menu"
    "form" "dual" "return" "const" "pub")
  "The words Slant reserves.")

(defconst slant-gone-words
  '("if" "else" "true" "false")
  "Words the lexer still knows only so the parser can refuse them.
There is no `if' — a choice is a `match' — and the prelude's `Bool' has
the variants `True' and `False'.")

(defconst slant-builtin-types
  '("i32" "i64" "u32" "u64" "char" "unit")
  "The base types written in lower case.")

(defconst slant--name "[[:alpha:]_][[:alnum:]_]*"
  "An identifier.")

(defconst slant--lower-name "[[:lower:]_][[:alnum:]_]*"
  "An identifier that names a value rather than a type or a variant.")

(defconst slant-font-lock-keywords
  `(;; `let+' and `let-' are modes of `let', and the sign touches it.
    ("\\_<let\\_>[+-]?" . font-lock-keyword-face)
    (,(regexp-opt slant-keywords 'symbols) . font-lock-keyword-face)
    (,(regexp-opt slant-gone-words 'symbols) . font-lock-warning-face)
    ;; `_ => forward', a handler's last clause.
    ("\\_<_\\s-*=>\\s-*\\(forward\\)\\_>" 1 font-lock-keyword-face)
    ;; What a declaration names.
    (,(concat "\\_<\\(?:fn\\|command\\)\\s-+\\(" slant--name "\\)")
     1 font-lock-function-name-face)
    (,(concat "\\_<\\(?:data\\|enum\\|menu\\|form\\|trait\\|effect\\)\\s-+\\("
              slant--name "\\)")
     1 font-lock-type-face)
    (,(concat "\\_<mod\\s-+\\(" slant--name "\\)") 1 font-lock-constant-face)
    (,(concat "\\_<const\\s-+\\(" slant--name "\\)") 1 font-lock-constant-face)
    ;; The builtins beneath the prelude's operators: `__add'.
    ("\\_<__[[:alnum:]_]+\\_>" . font-lock-builtin-face)
    (,(regexp-opt slant-builtin-types 'symbols) . font-lock-type-face)
    ;; A path's modules, `list::' in `list::List::Cons'.
    (,(concat "\\_<\\(" slant--lower-name "\\)::") 1 font-lock-constant-face)
    ;; A constant, then anything else capitalised: a type, a trait, an
    ;; effect, a variant.  Two capitals are an effect or a type — `IO' —
    ;; so a constant is three or more: `MAX', `OPEN_BRACKET'.
    ("\\_<[[:upper:]][[:upper:][:digit:]_]\\{2,\\}\\_>" . font-lock-constant-face)
    ("\\_<[[:upper:]][[:alnum:]_]*\\_>" . font-lock-type-face)
    ;; Binders: a `let', a parameter or field label `name:', and a chain
    ;; or arm binder, `x => e' and `k <= e'.
    (,(concat "\\_<let\\_>[+-]?\\s-+\\(" slant--lower-name "\\)\\_>")
     1 font-lock-variable-name-face)
    (,(concat "\\_<\\(" slant--lower-name "\\)\\s-*:[^:]")
     1 font-lock-variable-name-face)
    (,(concat "\\_<\\(" slant--lower-name "\\)\\s-*\\(?:=>\\|<=\\)")
     1 font-lock-variable-name-face)
    ;; A request or a projection by name, `.item(k)' and `cfg.name'; and an
    ;; alternative by position, `::0(v)'.
    (,(concat "\\.\\(" slant--lower-name "\\)\\_>") 1 font-lock-function-name-face)
    ("\\(?:^\\|[^[:alnum:]_]\\)\\(::[0-9]+\\)" 1 font-lock-constant-face))
  "Highlighting for `slant-mode'.")

;;; Indentation

(defun slant--code-line-p ()
  "Return non-nil if the current line has code: not blank, not only a comment."
  (save-excursion
    (back-to-indentation)
    (not (or (eolp) (looking-at-p "//") (nth 4 (syntax-ppss))))))

(defun slant--chain-indentation (open fallback)
  "The column for a line that opens with `|', a chain's next stage.
It lines up with the stage above it, or sits one level in from the line
the chain began on.  OPEN is the bracket the line is inside, and
FALLBACK the column when no line above shares it."
  (save-excursion
    (let (column)
      (while (and (not column)
                  (zerop (forward-line -1))
                  (or (null open) (> (point) open)))
        (when (and (slant--code-line-p)
                   (eq (nth 1 (syntax-ppss (line-beginning-position))) open))
          (back-to-indentation)
          (setq column (if (eq (char-after) ?|)
                           (current-column)
                         (+ (current-column) slant-indent-offset)))))
      (or column fallback))))

(defun slant--angle-indentation (open)
  "The indentation of the line whose trailing `<' is still open, or nil.
`<' and `>' are not brackets to the syntax table, but `slc fmt' breaks a
long `impl<' … `>' one parameter per line, so the line above is asked.
OPEN is the bracket the current line is inside."
  (save-excursion
    (let (column closed)
      (while (and (not column) (not closed)
                  (zerop (forward-line -1))
                  (or (null open) (> (point) open)))
        (when (and (slant--code-line-p)
                   (eq (nth 1 (syntax-ppss (line-beginning-position))) open))
          (back-to-indentation)
          (cond ((eq (char-after) ?>) (setq closed t))
                ((looking-at-p ".*<\\s-*$") (setq column (current-column))))))
      column)))

(defun slant--calculate-indentation ()
  "The column the current line belongs at, or nil to leave it alone."
  (save-excursion
    (back-to-indentation)
    (let* ((state (syntax-ppss))
           (open (nth 1 state)))
      (cond
       ;; Inside a string or a block comment: as written.
       ((or (nth 3 state) (nth 4 state)) nil)
       ;; A closing bracket lines up with the line that opened it.
       ((looking-at-p "[])}]")
        (if open (save-excursion (goto-char open) (current-indentation)) 0))
       ;; Inside a broken `<' … `>': a level in, and the `>' back out.
       ((slant--angle-indentation open)
        (+ (slant--angle-indentation open)
           (if (eq (char-after) ?>) 0 slant-indent-offset)))
       (t
        (let ((inside (if open
                          (save-excursion
                            (goto-char open)
                            (+ (current-indentation) slant-indent-offset))
                        0)))
          (if (and (eq (char-after) ?|) (not (eq (char-after (1+ (point))) ?|)))
              ;; With no stage above it inside the bracket, the chain began
              ;; on the bracket's own line: `(<v | f' … `| g)'.
              (slant--chain-indentation open inside)
            inside)))))))

(defun slant-indent-line ()
  "Indent the current line as `slc fmt' would."
  (interactive)
  (let ((column (slant--calculate-indentation))
        (offset (- (current-column) (current-indentation))))
    (if (null column)
        'noindent
      (indent-line-to column)
      (when (> offset 0)
        (forward-char offset)))))

;;; Formatting

(defun slant-format-buffer ()
  "Format the buffer with `slc fmt'.
The formatter refuses source that does not parse, and its message is
shown; the buffer is then left as it was."
  (interactive)
  (let ((source (make-temp-file "slant-fmt" nil ".sl"))
        (output (generate-new-buffer " *slc fmt*"))
        (errors (make-temp-file "slant-fmt-errors")))
    (unwind-protect
        (progn
          (let ((coding-system-for-write 'utf-8-unix))
            (write-region nil nil source nil 'silent))
          (let ((status (let ((coding-system-for-read 'utf-8-unix))
                          (call-process slant-slc-command nil (list output errors) nil
                                        "fmt" "--stdout" source))))
            (if (eq status 0)
                (progn
                  (replace-buffer-contents output)
                  (message "Formatted"))
              (message "slc fmt: %s"
                       (with-temp-buffer
                         (insert-file-contents errors)
                         (string-trim
                          (replace-regexp-in-string
                           (regexp-quote source) (buffer-name) (buffer-string))))))))
      (kill-buffer output)
      (delete-file source)
      (delete-file errors))))

(defun slant--format-before-save ()
  "Format the buffer when `slant-format-on-save' asks for it."
  (when slant-format-on-save
    (slant-format-buffer)))

;;; The mode

(defvar slant-imenu-generic-expression
  `(("Functions" ,(concat "^\\s-*\\(?:pub\\s-+\\)?fn\\s-+\\(" slant--name "\\)") 1)
    ("Commands" ,(concat "^\\s-*\\(?:pub\\s-+\\)?command\\s-+\\(" slant--name "\\)") 1)
    ("Types" ,(concat "^\\s-*\\(?:pub\\s-+\\)?\\(?:data\\|enum\\|menu\\|form\\)\\s-+\\("
                      slant--name "\\)")
     1)
    ("Traits" ,(concat "^\\s-*\\(?:pub\\s-+\\)?trait\\s-+\\(" slant--name "\\)") 1)
    ("Effects" ,(concat "^\\s-*\\(?:pub\\s-+\\)?effect\\s-+\\(" slant--name "\\)") 1)
    ("Modules" ,(concat "^\\s-*\\(?:pub\\s-+\\)?mod\\s-+\\(" slant--name "\\)") 1))
  "The declarations `imenu' lists.")

(defvar slant-mode-map
  (let ((map (make-sparse-keymap)))
    (define-key map (kbd "C-c C-f") #'slant-format-buffer)
    map)
  "Keymap for `slant-mode'.")

;;;###autoload
(define-derived-mode slant-mode prog-mode "Slant"
  "Major mode for editing Slant source.

\\{slant-mode-map}"
  :syntax-table slant-mode-syntax-table
  (setq-local font-lock-defaults '(slant-font-lock-keywords))
  (setq-local syntax-propertize-function slant--syntax-propertize)
  (setq-local comment-start "// ")
  (setq-local comment-end "")
  (setq-local comment-start-skip "\\(?://+\\|/\\*+\\)\\s-*")
  (setq-local comment-use-syntax t)
  (setq-local indent-line-function #'slant-indent-line)
  (setq-local indent-tabs-mode nil)
  (setq-local electric-indent-chars (append '(?\} ?\) ?\] ?|) electric-indent-chars))
  (setq-local imenu-generic-expression slant-imenu-generic-expression)
  (setq-local fill-column 100)
  (add-hook 'before-save-hook #'slant--format-before-save nil t))

;;;###autoload
(add-to-list 'auto-mode-alist '("\\.sl\\'" . slant-mode))

(provide 'slant-mode)

;;; slant-mode.el ends here
