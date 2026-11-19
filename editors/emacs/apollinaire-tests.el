;;; apollinaire-tests.el --- Tests for apollinaire.el -*- lexical-binding: t; -*-

;; Copyright 2026 Ryan Duguid
;; SPDX-License-Identifier: MIT

;;; Commentary:

;; Run against a built apollinaire:
;; APOLLINAIRE=<binary> emacs --batch -Q -L editors/emacs -l apollinaire-tests \
;;   -f ert-run-tests-batch-and-exit

;;; Code:

(require 'apollinaire)
(require 'cl-lib)
(require 'ert)

(defvar apollinaire-tests-bin (or (getenv "APOLLINAIRE")
                                  (error "Set APOLLINAIRE to the apollinaire binary")))

(defun apollinaire-tests-cli (&rest args)
  "What the CLI prints for ARGS."
  (with-temp-buffer
    (should (eql (apply #'call-process apollinaire-tests-bin nil t nil args) 0))
    (buffer-string)))

(defmacro apollinaire-tests-with (text &rest body)
  "Run BODY in a buffer holding TEXT, with point at its end."
  (declare (indent 1))
  `(let ((apollinaire-program apollinaire-tests-bin))
     (with-temp-buffer
       (insert ,text)
       ,@body)))

(ert-deftest apollinaire-inserts-at-point ()
  (apollinaire-tests-with "one\ntwo\n"
    (goto-char (point-min))
    (forward-line 1)
    (apollinaire "art cat")
    (should (equal (buffer-string)
                   (concat "one\n" (apollinaire-tests-cli "art" "cat") "two\n")))))

(ert-deftest apollinaire-passes-quoted-arguments-whole ()
  (apollinaire-tests-with ""
    (apollinaire "say \"hello friend\" --width 20")
    (should (equal (buffer-string)
                   (apollinaire-tests-cli "say" "hello friend" "--width" "20")))))

(ert-deftest apollinaire-filters-a-region ()
  (apollinaire-tests-with "keep\nItem,Qty\nLobster,28\nkeep\n"
    (goto-char (point-min))
    (let ((start (line-beginning-position 2))
          (end (line-beginning-position 4)))
      (apollinaire "table - --separator ," start end))
    (should (equal (buffer-string)
                   (concat "keep\n"
                           "+---------+-----+\n"
                           "| Item    | Qty |\n"
                           "+---------+-----+\n"
                           "| Lobster | 28  |\n"
                           "+---------+-----+\n"
                           "keep\n")))))

(ert-deftest apollinaire-failure-leaves-the-buffer ()
  (apollinaire-tests-with "src\n\tlib.rs\n"
    (let ((err (should-error (apollinaire "tree -" (point-min) (point-max))
                             :type 'user-error)))
      (should (string-match-p "tab" (cadr err))))
    (should (equal (buffer-string) "src\n\tlib.rs\n"))))

(ert-deftest apollinaire-output-to-a-file-leaves-the-region ()
  (let ((file (concat (make-temp-name (expand-file-name "apollinaire" temporary-file-directory))
                      ".txt")))
    (unwind-protect
        (apollinaire-tests-with "a,b\n1,2\n"
          (let (said)
            (cl-letf (((symbol-function 'message)
                       (lambda (format &rest args) (setq said (apply #'format format args)))))
              (apollinaire (concat "table - --separator , --output " file)
                           (point-min) (point-max)))
            (should (equal said "apollinaire printed nothing, so the buffer is unchanged")))
          (should (equal (buffer-string) "a,b\n1,2\n"))
          (should (equal (with-temp-buffer
                           (insert-file-contents file)
                           (buffer-string))
                         (concat "+---+---+\n| a | b |\n+---+---+\n"
                                 "| 1 | 2 |\n+---+---+\n"))))
      (when (file-exists-p file)
        (delete-file file)))))

(ert-deftest apollinaire-missing-program-leaves-the-buffer ()
  (apollinaire-tests-with "x\n"
    (let ((apollinaire-program "/nonexistent/apollinaire"))
      (should-error (apollinaire "art cat") :type 'file-missing))
    (should (equal (buffer-string) "x\n"))))

(ert-deftest apollinaire-check-lists-problems-for-next-error ()
  ;; A tab, then 2 letters and a right single quotation mark (U+2019).
  (let ((file (make-temp-file "apollinaire" nil ".txt"
                              (concat "first line\nabcde\na\tbc" (string #x2019) "\n")))
        (apollinaire-program apollinaire-tests-bin))
    (unwind-protect
        (with-current-buffer (find-file-noselect file)
          (goto-char (point-min))
          (apollinaire-check (line-beginning-position 2) (point-max))
          (should (equal (with-current-buffer "*apollinaire-check*"
                           (buffer-substring-no-properties (point-min) (point-max)))
                         (concat "apollinaire: 2 problems in stdin:\n"
                                 file ":3:2: a tab; use spaces\n"
                                 file ":3:5: U+2019 is not ASCII; use '\n")))
          (next-error)
          (should (equal (list (line-number-at-pos) (current-column)) '(3 1)))
          (should (eq (char-after) ?\t))
          ;; check counts the tab as one character, not as the screen columns it spans.
          (next-error)
          (should (equal (line-number-at-pos) 3))
          (should (eq (char-after) #x2019))
          (set-buffer-modified-p nil)
          (kill-buffer))
      (delete-file file)
      (kill-buffer "*apollinaire-check*"))))

(ert-deftest apollinaire-check-reports-valid-art ()
  (apollinaire-tests-with "ab\ncd\n"
    (let (said)
      (cl-letf (((symbol-function 'message)
                 (lambda (format &rest args) (setq said (apply #'format format args)))))
        (apollinaire-check))
      (should (equal said "Valid ASCII: 2 x 2, 1 frame(s), 100 ms")))))

(ert-deftest apollinaire-check-reports-an-error-without-a-line ()
  (apollinaire-tests-with "{\"version\": 1}\n"
    (let ((err (should-error (apollinaire-check) :type 'user-error)))
      (should (string-prefix-p "apollinaire: " (cadr err))))))

(provide 'apollinaire-tests)

;;; apollinaire-tests.el ends here
