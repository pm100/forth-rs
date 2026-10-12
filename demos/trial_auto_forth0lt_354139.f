\ ============================================================
\  Negative number check
\
\  Checks whether a number on the stack is negative, printing
\  a message either way. Exercised here for one negative value
\  (-5) and one non-negative value (5).
\ ============================================================

: negative? ( n -- )
    dup 0 < if
        . ." is negative" cr
    else
        . ." is not negative" cr
    then
;

-5 negative?
5 negative?
