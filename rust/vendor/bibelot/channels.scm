;; bibelot -- floating desktop widgets written in Rust, GTK4 and GNU Guile Scheme
;;
;; SPDX-License-Identifier: MIT
;;
;; The package needs Rust 1.92.  Build it with
;;   guix time-machine --channels=channels.scm -- build -f guix.scm
;; so the toolchain comes from this pin rather than the host channel.

(list (channel
        (name 'guix)
        (url "https://git.guix.gnu.org/guix.git")
        (branch "master")
        (commit "5cea8de04500ba7904e02ee7b53178fb06856e13")
        (introduction
         (make-channel-introduction "9edb3f66fd807b096b48283debdcddccfea34bad"
          (openpgp-fingerprint
           "BBB0 2DDF 2CEA F6A8 0D1D  E643 A2A0 6DF2 A33A 54FA")))))
