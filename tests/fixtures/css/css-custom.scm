;; A bar that brings its own rules: the material first, #:css verbatim last.
(configure-shell! #:bars (list (bar #:name 'main #:theme 'ember
                                     #:css "window#whelk-main .whelk-module { border-radius: 4px; }
                                            window#whelk-main .whelk-label { font-weight: 600; }")))
