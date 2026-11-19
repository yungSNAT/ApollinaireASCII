;;; apollinaire.el --- Run the apollinaire ASCII art CLI -*- lexical-binding: t; -*-

;; Copyright 2026 Ryan Duguid
;; SPDX-License-Identifier: MIT

;; Author: Ryan Duguid
;; URL: https://github.com/yungSNAT/ApollinaireASCII
;; Version: 0.1.0
;; Package-Requires: ((emacs "29.1"))
;; Keywords: tools

;;; Commentary:

;; Commands that run the apollinaire CLI, which draws art, lettering, speech bubbles,
;; tables, trees and flowcharts in printable ASCII.
;;
;; M-x apollinaire inserts what a command prints at point, or, with an active region,
;; sends the region on stdin and replaces it with the output: write - where the command
;; takes its input, as in "table - --separator ,".  A command that fails leaves the
;; buffer as it was and shows its error.
;;
;; M-x apollinaire-check lists the problems apollinaire check finds in the buffer, or the
;; active region, in the *apollinaire-check* buffer, where `next-error' visits each one.

;;; Code:

(require 'compile)

(defgroup apollinaire nil
  "Run the apollinaire ASCII art CLI."
  :group 'tools
  :prefix "apollinaire-")

(defcustom apollinaire-program "apollinaire"
  "The apollinaire program: a name on `exec-path' or a file name."
  :type 'string)

(defvar apollinaire-history nil
  "Arguments given to `apollinaire'.")

(defun apollinaire--run (args &optional input)
  "Run `apollinaire-program' with ARGS, sending the string INPUT on stdin if given.
Return a list of the exit status, stdout and stderr."
  (let ((stderr (make-temp-file "apollinaire"))
        (coding-system-for-write 'utf-8-unix)
        (coding-system-for-read 'utf-8-unix))
    (unwind-protect
        (with-temp-buffer
          (let ((status (if input
                            (apply #'call-process-region input nil apollinaire-program
                                   nil (list t stderr) nil args)
                          (apply #'call-process apollinaire-program
                                 nil (list t stderr) nil args))))
            (list status
                  (buffer-string)
                  (with-temp-buffer
                    (insert-file-contents stderr)
                    (buffer-string)))))
      (delete-file stderr))))

;;;###autoload
(defun apollinaire (args &optional start end)
  "Run apollinaire with ARGS, a string split as a shell splits it.
With START and END, interactively the active region, send that text on
stdin and replace it with the output; otherwise insert the output at
point.  Write - where the command takes its input, as in
\"table - --separator ,\".  A command that fails leaves the buffer as
it was and signals its error."
  (interactive
   (cons (read-string "apollinaire " nil 'apollinaire-history)
         (and (use-region-p) (list (region-beginning) (region-end)))))
  (let* ((input (and start (buffer-substring-no-properties start end)))
         (result (apollinaire--run (split-string-shell-command args) input)))
    (unless (eql (nth 0 result) 0)
      (user-error "%s" (string-trim (nth 2 result))))
    ;; Such as with --output: replacing the region with nothing would delete it.
    (if (equal (nth 1 result) "")
        (message "apollinaire printed nothing, so the buffer is unchanged")
      (when start
        (delete-region start end)
        (goto-char start))
      (insert (nth 1 result)))))

;;;###autoload
(defun apollinaire-check (&optional start end)
  "Check the text from START to END, or the whole buffer, as apollinaire art.
Interactively, check the active region.  List each problem in the
*apollinaire-check* buffer, where `next-error' visits it in a buffer
visiting a file."
  (interactive (and (use-region-p) (list (region-beginning) (region-end))))
  (let* ((start (or start (point-min)))
         (end (or end (point-max)))
         (offset (1- (line-number-at-pos start t)))
         ;; shortcut: problems point at the buffer's file, so `next-error' cannot visit
         ;; a buffer with none; markers in the listing would reach any buffer.
         (name (or buffer-file-name (buffer-name)))
         (directory default-directory)
         (result (apollinaire--run '("check" "-")
                                   (buffer-substring-no-properties start end)))
         (stderr (nth 2 result))
         (problems nil))
    (if (eql (nth 0 result) 0)
        (message "%s" (string-trim (nth 1 result)))
      (with-temp-buffer
        (insert stderr)
        (goto-char (point-min))
        (while (re-search-forward
                "^  line \\([0-9]+\\), column \\([0-9]+\\): \\(.*\\)$" nil t)
          (push (format "%s:%d:%s: %s\n" name
                        (+ offset (string-to-number (match-string 1)))
                        (match-string 2) (match-string 3))
                problems)))
      (unless problems
        (user-error "%s" (string-trim stderr)))
      (with-current-buffer (get-buffer-create "*apollinaire-check*")
        (let ((inhibit-read-only t))
          (erase-buffer)
          (setq default-directory directory)
          (insert (car (split-string stderr "\n")) "\n")
          (apply #'insert (nreverse problems)))
        (compilation-mode)
        ;; check counts characters, not screen columns.
        (setq-local compilation-error-screen-columns nil)
        (goto-char (point-min))
        (setq next-error-last-buffer (current-buffer))
        (display-buffer (current-buffer)))
      (message "%s" (car (split-string stderr "\n"))))))

(provide 'apollinaire)

;;; apollinaire.el ends here
