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

(defvar-keymap apollinaire-check-mode-map
  :doc "Keys in the *apollinaire-check* buffer."
  :parent special-mode-map
  "RET" #'apollinaire-check-visit
  "n" #'next-error-no-select
  "p" #'previous-error-no-select)

(define-derived-mode apollinaire-check-mode special-mode "Apollinaire-Check"
  "The problems `apollinaire-check' found, one a line after the first.
RET visits the problem on its line, and `next-error' visits each in turn."
  (setq next-error-function #'apollinaire-check--next-error))

(defvar-local apollinaire-check--line 1
  "The line of the problem visited last, or 1, the count's line, before any.")

(defun apollinaire-check-visit ()
  "Visit the problem on this line."
  (interactive)
  (let ((place (get-text-property (line-beginning-position) 'apollinaire-check)))
    (unless (and place (buffer-live-p (marker-buffer place)))
      (user-error "No problem to visit on this line"))
    (setq apollinaire-check--line (line-number-at-pos))
    (pop-to-buffer (marker-buffer place))
    (goto-char place)))

(defun apollinaire-check--next-error (n &optional reset)
  "Visit the Nth problem after the last one visited, or the first with RESET.
Point in this buffer is not used, as `next-error' runs this away from
its window."
  (let ((line (+ (if reset 1 apollinaire-check--line) n)))
    ;; The buffer ends with a line feed.
    (unless (<= 2 line (1- (line-number-at-pos (point-max))))
      (user-error "No more problems"))
    (goto-char (point-min))
    (forward-line (1- line))
    (let ((window (get-buffer-window)))
      (when window
        (set-window-point window (point)))))
  (apollinaire-check-visit))

(defun apollinaire-check--place (start line column)
  "A marker at LINE and COLUMN, counted in characters, of text from START."
  (save-excursion
    (goto-char start)
    (when (> line 1)
      (forward-line (1- line)))
    ;; A short row's problem sits one past its last character.
    (goto-char (min (+ (point) (1- column)) (line-end-position)))
    (point-marker)))

;;;###autoload
(defun apollinaire-check (&optional start end)
  "Check the text from START to END, or the whole buffer, as apollinaire art.
Interactively, check the active region.  List each problem in the
*apollinaire-check* buffer, where `next-error' visits it."
  (interactive (and (use-region-p) (list (region-beginning) (region-end))))
  (let* ((start (or start (point-min)))
         (end (or end (point-max)))
         (result (apollinaire--run '("check" "-")
                                   (buffer-substring-no-properties start end)))
         (stderr (nth 2 result))
         (header (car (split-string stderr "\n")))
         (from 0)
         (lines nil))
    (if (eql (nth 0 result) 0)
        (message "%s" (string-trim (nth 1 result)))
      (while (string-match "^  line \\([0-9]+\\), column \\([0-9]+\\): \\(.*\\)$"
                           stderr from)
        (setq from (match-end 0))
        (let* ((what (match-string 3 stderr))
               (place (apollinaire-check--place
                       start (string-to-number (match-string 1 stderr))
                       (string-to-number (match-string 2 stderr)))))
          (push (propertize
                 (save-excursion
                   (goto-char place)
                   (format "%s:%d:%d: %s\n" (buffer-name) (line-number-at-pos nil t)
                           (1+ (- place (line-beginning-position))) what))
                 'apollinaire-check place)
                lines)))
      (unless lines
        (user-error "%s" (string-trim stderr)))
      (let ((listing (get-buffer-create "*apollinaire-check*")))
        (with-current-buffer listing
          (let ((inhibit-read-only t))
            (erase-buffer)
            (insert header "\n")
            (apply #'insert (nreverse lines)))
          (apollinaire-check-mode)
          (goto-char (point-min)))
        (setq next-error-last-buffer listing)
        (display-buffer listing))
      (message "%s" header))))

(provide 'apollinaire)

;;; apollinaire.el ends here
