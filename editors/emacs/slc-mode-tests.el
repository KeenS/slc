;;; slc-mode-tests.el --- Tests for slc-mode -*- lexical-binding: t; -*-

;;; Commentary:

;; From the repository root:
;;
;;   emacs -Q --batch -L editors/emacs -l editors/emacs/slc-mode-tests.el \
;;         -f ert-run-tests-batch-and-exit

;;; Code:

(require 'ert)
(require 'imenu)
(require 'slc-mode)

(defconst slc-tests--root
  (expand-file-name "../../" (file-name-directory (or load-file-name buffer-file-name)))
  "The repository.")

(defmacro slc-tests--with (source &rest body)
  "Run BODY in a fontified `slc-mode' buffer holding SOURCE."
  (declare (indent 1))
  `(with-temp-buffer
     (insert ,source)
     (slc-mode)
     (font-lock-ensure)
     (goto-char (point-min))
     ,@body))

(defun slc-tests--face (text)
  "The face on the first character of TEXT's next occurrence."
  (search-forward text)
  (get-text-property (match-beginning 0) 'face))

(ert-deftest slc-keywords-names-and-types-are-highlighted ()
  (slc-tests--with "pub proc nth<+T>(xs: List<T>) | (found: i64) / {IO} { let+ y = MAX; }"
    (should (eq (slc-tests--face "pub") 'font-lock-keyword-face))
    (should (eq (slc-tests--face "proc") 'font-lock-keyword-face))
    (should (eq (slc-tests--face "nth") 'font-lock-function-name-face))
    (should (eq (slc-tests--face "xs") 'font-lock-variable-name-face))
    (should (eq (slc-tests--face "List") 'font-lock-type-face))
    (should (eq (slc-tests--face "i64") 'font-lock-type-face))
    (should (eq (slc-tests--face "IO") 'font-lock-type-face))
    (should (eq (slc-tests--face "let+") 'font-lock-keyword-face))
    (should (eq (slc-tests--face "y") 'font-lock-variable-name-face))
    (should (eq (slc-tests--face "MAX") 'font-lock-constant-face))))

(ert-deftest slc-binders-paths-and-builtins-are-highlighted ()
  (slc-tests--with "func f() -> i64 { <(a, 1) | __add | x => (x, 2) | list::sum | k <= cfg.name }"
    (should (eq (slc-tests--face "__add") 'font-lock-builtin-face))
    (should (eq (slc-tests--face "x =>") 'font-lock-variable-name-face))
    (should (eq (slc-tests--face "list") 'font-lock-constant-face))
    (should (eq (slc-tests--face "k <=") 'font-lock-variable-name-face))
    (should (eq (slc-tests--face "name") 'font-lock-function-name-face))))

(ert-deftest slc-if-and-true-are-ordinary-names ()
  (slc-tests--with "func f() -> Bool { if true }"
    (should (eq (slc-tests--face "if") nil))
    (should (eq (slc-tests--face "true") nil))))

(ert-deftest slc-a-quote-in-a-char-literal-opens-no-string ()
  (slc-tests--with "def QUOTE: char = '\"';\ndef TICK: char = '\\'';\nfunc f() -> i64 { 1 }"
    (should (eq (slc-tests--face "'\"'") 'font-lock-string-face))
    (should (eq (slc-tests--face "'\\''") 'font-lock-string-face))
    (should (eq (slc-tests--face "func") 'font-lock-keyword-face))
    (should-not (nth 3 (syntax-ppss (point-max))))))

(ert-deftest slc-comments-nest-and-a-slash-alone-is-no-comment ()
  (slc-tests--with "func f() -> i64 / {IO} { 1 } // line\n/* outer /* inner */ still */ func g() -> i64 { 2 }"
    (should (eq (slc-tests--face "IO") 'font-lock-type-face))
    (should (eq (slc-tests--face "line") 'font-lock-comment-face))
    (should (eq (slc-tests--face "still") 'font-lock-comment-face))
    (should (eq (slc-tests--face "g") 'font-lock-function-name-face))))

(ert-deftest slc-indentation-is-a-level-per-bracket-and-a-chain-one-in ()
  (let ((laid-out "func f(text: String) -> (,) / {IO} {
    let n = of text {
        \"\" => 0,
        _ => {
            <text | str_len
        },
    };
    <(\"wrote \", n)
        | add
        | x => (x, \" characters\") | add
        | println
}
"))
    (slc-tests--with (replace-regexp-in-string "^ +" "" laid-out)
      (indent-region (point-min) (point-max))
      (should (equal (buffer-string) laid-out)))))

(ert-deftest slc-indentation-agrees-with-slc-fmt ()
  "Every source `slc fmt' keeps formatted is laid out as the mode indents."
  (let ((files (append
                (directory-files-recursively
                 (expand-file-name "examples" slc-tests--root) "\\.sl\\'")
                (directory-files-recursively
                 (expand-file-name "crates/slc-driver/src" slc-tests--root) "\\.sl\\'"))))
    (should (> (length files) 50))
    (dolist (file files)
      (with-temp-buffer
        (insert-file-contents file)
        (slc-mode)
        (let ((formatted (buffer-string))
              (inhibit-message t))
          (indent-region (point-min) (point-max))
          (ert-info ((file-name-nondirectory file) :prefix "file: ")
            (should (equal (buffer-string) formatted))))))))

(ert-deftest slc-imenu-lists-declarations ()
  (slc-tests--with "mod m {\n    pub func length() -> i64 { 0 }\n    proc go | (exit: i32) { <0 | exit> }\n    enum Bool { False, True }\n}\n"
    (let ((index (funcall imenu-create-index-function)))
      (should (assoc "length" (cdr (assoc "Functions" index))))
      (should (assoc "go" (cdr (assoc "Commands" index))))
      (should (assoc "Bool" (cdr (assoc "Types" index))))
      (should (assoc "m" (cdr (assoc "Modules" index)))))))

(ert-deftest slc-format-buffer-runs-slc-fmt ()
  (let ((slc-command (expand-file-name "target/debug/slc" slc-tests--root)))
    (skip-unless (file-executable-p slc-command))
    (slc-tests--with "proc main|(exit:i32)/{IO}{\n<0|exit>}\n"
      (let ((inhibit-message t))
        (slc-format-buffer))
      (should (equal (buffer-string)
                     "proc main | (exit: i32) / {IO} {\n    <0 | exit>\n}\n")))
    ;; Source that does not parse is left as it was.
    (slc-tests--with "proc main | (exit: i32) { if }\n"
      (let ((inhibit-message t))
        (slc-format-buffer))
      (should (equal (buffer-string) "proc main | (exit: i32) { if }\n")))))

(provide 'slc-mode-tests)

;;; slc-mode-tests.el ends here
