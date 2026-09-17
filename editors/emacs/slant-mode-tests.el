;;; slant-mode-tests.el --- Tests for slant-mode -*- lexical-binding: t; -*-

;;; Commentary:

;; From the repository root:
;;
;;   emacs -Q --batch -L editors/emacs -l editors/emacs/slant-mode-tests.el \
;;         -f ert-run-tests-batch-and-exit

;;; Code:

(require 'ert)
(require 'imenu)
(require 'slant-mode)

(defconst slant-tests--root
  (expand-file-name "../../" (file-name-directory (or load-file-name buffer-file-name)))
  "The repository.")

(defmacro slant-tests--with (source &rest body)
  "Run BODY in a fontified `slant-mode' buffer holding SOURCE."
  (declare (indent 1))
  `(with-temp-buffer
     (insert ,source)
     (slant-mode)
     (font-lock-ensure)
     (goto-char (point-min))
     ,@body))

(defun slant-tests--face (text)
  "The face on the first character of TEXT's next occurrence."
  (search-forward text)
  (get-text-property (match-beginning 0) 'face))

(ert-deftest slant-keywords-names-and-types-are-highlighted ()
  (slant-tests--with "pub command nth<+T>(xs: List<T>) | (found: i64) / {IO} { let+ y = MAX; }"
    (should (eq (slant-tests--face "pub") 'font-lock-keyword-face))
    (should (eq (slant-tests--face "command") 'font-lock-keyword-face))
    (should (eq (slant-tests--face "nth") 'font-lock-function-name-face))
    (should (eq (slant-tests--face "xs") 'font-lock-variable-name-face))
    (should (eq (slant-tests--face "List") 'font-lock-type-face))
    (should (eq (slant-tests--face "i64") 'font-lock-type-face))
    (should (eq (slant-tests--face "IO") 'font-lock-type-face))
    (should (eq (slant-tests--face "let+") 'font-lock-keyword-face))
    (should (eq (slant-tests--face "y") 'font-lock-variable-name-face))
    (should (eq (slant-tests--face "MAX") 'font-lock-constant-face))))

(ert-deftest slant-binders-paths-and-builtins-are-highlighted ()
  (slant-tests--with "fn f() -> i64 { <(a, 1) | __add | x => (x, 2) | list::sum | k <= cfg.name }"
    (should (eq (slant-tests--face "__add") 'font-lock-builtin-face))
    (should (eq (slant-tests--face "x =>") 'font-lock-variable-name-face))
    (should (eq (slant-tests--face "list") 'font-lock-constant-face))
    (should (eq (slant-tests--face "k <=") 'font-lock-variable-name-face))
    (should (eq (slant-tests--face "name") 'font-lock-function-name-face))))

(ert-deftest slant-words-the-parser-refuses-are-warnings ()
  (slant-tests--with "fn f() -> Bool { if true }"
    (should (eq (slant-tests--face "if") 'font-lock-warning-face))
    (should (eq (slant-tests--face "true") 'font-lock-warning-face))))

(ert-deftest slant-a-quote-in-a-char-literal-opens-no-string ()
  (slant-tests--with "const QUOTE: char = '\"';\nconst TICK: char = '\\'';\nfn f() -> i64 { 1 }"
    (should (eq (slant-tests--face "'\"'") 'font-lock-string-face))
    (should (eq (slant-tests--face "'\\''") 'font-lock-string-face))
    (should (eq (slant-tests--face "fn") 'font-lock-keyword-face))
    (should-not (nth 3 (syntax-ppss (point-max))))))

(ert-deftest slant-comments-nest-and-a-slash-alone-is-no-comment ()
  (slant-tests--with "fn f() -> i64 / {IO} { 1 } // line\n/* outer /* inner */ still */ fn g() -> i64 { 2 }"
    (should (eq (slant-tests--face "IO") 'font-lock-type-face))
    (should (eq (slant-tests--face "line") 'font-lock-comment-face))
    (should (eq (slant-tests--face "still") 'font-lock-comment-face))
    (should (eq (slant-tests--face "g") 'font-lock-function-name-face))))

(ert-deftest slant-indentation-is-a-level-per-bracket-and-a-chain-one-in ()
  (let ((laid-out "fn f(text: String) -> (,) / {IO} {
    let n = match text {
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
    (slant-tests--with (replace-regexp-in-string "^ +" "" laid-out)
      (indent-region (point-min) (point-max))
      (should (equal (buffer-string) laid-out)))))

(ert-deftest slant-indentation-agrees-with-slc-fmt ()
  "Every source `slc fmt' keeps formatted is laid out as the mode indents."
  (let ((files (append
                (directory-files-recursively
                 (expand-file-name "examples" slant-tests--root) "\\.sl\\'")
                (directory-files-recursively
                 (expand-file-name "crates/slc-driver/src" slant-tests--root) "\\.sl\\'"))))
    (should (> (length files) 50))
    (dolist (file files)
      (with-temp-buffer
        (insert-file-contents file)
        (slant-mode)
        (let ((formatted (buffer-string))
              (inhibit-message t))
          (indent-region (point-min) (point-max))
          (ert-info ((file-name-nondirectory file) :prefix "file: ")
            (should (equal (buffer-string) formatted))))))))

(ert-deftest slant-imenu-lists-declarations ()
  (slant-tests--with "mod m {\n    pub fn length() -> i64 { 0 }\n    command go | (exit: i32) { <0 | exit> }\n    enum Bool { False, True }\n}\n"
    (let ((index (funcall imenu-create-index-function)))
      (should (assoc "length" (cdr (assoc "Functions" index))))
      (should (assoc "go" (cdr (assoc "Commands" index))))
      (should (assoc "Bool" (cdr (assoc "Types" index))))
      (should (assoc "m" (cdr (assoc "Modules" index)))))))

(ert-deftest slant-format-buffer-runs-slc-fmt ()
  (let ((slant-slc-command (expand-file-name "target/debug/slc" slant-tests--root)))
    (skip-unless (file-executable-p slant-slc-command))
    (slant-tests--with "command main|(exit:i32)/{IO}{\n<0|exit>}\n"
      (let ((inhibit-message t))
        (slant-format-buffer))
      (should (equal (buffer-string)
                     "command main | (exit: i32) / {IO} {\n    <0 | exit>\n}\n")))
    ;; Source that does not parse is left as it was.
    (slant-tests--with "command main | (exit: i32) { if }\n"
      (let ((inhibit-message t))
        (slant-format-buffer))
      (should (equal (buffer-string) "command main | (exit: i32) { if }\n")))))

(provide 'slant-mode-tests)

;;; slant-mode-tests.el ends here
