\ ============================================================
\  Animated Moire Pattern
\  (Forth.rs + dyncall FFI + SDL2)
\
\  Two line gratings are drawn every frame: a fixed vertical one
\  and a copy of it rotated by a slowly increasing angle. Where
\  the two sets of lines nearly align the pattern reads as
\  bright/sparse; where they interleave densely it reads as dark
\  - the classic moire fringe effect, continuously reshaping as
\  the rotation angle sweeps round. Both gratings' colours also
\  cycle smoothly through the spectrum over time, 180 degrees out
\  of phase with each other so they stay visually distinct as the
\  hues shift.
\
\  This Forth has no floating point, so rotation and colour use
\  fixed-point integer trigonometry: a table of sin(0..90 degrees)
\  scaled by 1000, folded by quadrant for the full circle, instead
\  of the radians + SIN/COS used by the BASIC version of this demo.
\
\  Press any key, or close the window, to quit.
\ ============================================================

\ --- SDL2 bindings via dyncall ---
extern: sdlinit   "C:\tools\SDL2.dll|SDL_Init|u32|i32|"
extern: sdlcwin   "C:\tools\SDL2.dll|SDL_CreateWindow|cstr,i32,i32,i32,i32,u32|ptr|"
extern: sdlcrend  "C:\tools\SDL2.dll|SDL_CreateRenderer|ptr,i32,u32|ptr|"
extern: sdlcolor  "C:\tools\SDL2.dll|SDL_SetRenderDrawColor|ptr,u8,u8,u8,u8|i32|"
extern: sdlblend  "C:\tools\SDL2.dll|SDL_SetRenderDrawBlendMode|ptr,u32|i32|"
extern: sdlrclr   "C:\tools\SDL2.dll|SDL_RenderClear|ptr|i32|"
extern: sdlline   "C:\tools\SDL2.dll|SDL_RenderDrawLine|ptr,i32,i32,i32,i32|i32|"
extern: sdlrpres  "C:\tools\SDL2.dll|SDL_RenderPresent|ptr|void|"
extern: sdldelay  "C:\tools\SDL2.dll|SDL_Delay|u32|void|"
extern: pollevent "C:\tools\SDL2.dll|SDL_PollEvent|*{i32,i32,i32,i32,i32,i32,i32,i32,i32,i32,i32,i32,i32,i32}|i32|"
extern: sdldrend  "C:\tools\SDL2.dll|SDL_DestroyRenderer|ptr|void|"
extern: sdldwin   "C:\tools\SDL2.dll|SDL_DestroyWindow|ptr|void|"
extern: sdlquit   "C:\tools\SDL2.dll|SDL_Quit||void|"

\ --- Constants ---
32 constant SDL_INIT_VIDEO
805240832 constant SDL_WINDOWPOS_CENTERED
2 constant SDL_RENDERER_ACCELERATED
1 constant SDL_BLENDMODE_BLEND
256 constant QUIT_EVT
768 constant KEY_EVT

\ --- Screen / grating geometry ---
900 constant W
700 constant H
450 constant CX          \ W / 2
350 constant CY          \ H / 2
6 constant SP            \ spacing between grating lines, in pixels
650 constant HALFLN      \ half of a line long enough to cover W,H when rotated
100 constant NMAX        \ half-count of lines per grating
-100 constant NMIN

\ --- Fixed-point sin(0..90 degrees) * 1000, quadrant-folded for a full circle ---
create sintab
0 , 17 , 35 , 52 , 70 , 87 , 105 , 122 , 139 , 156 ,
174 , 191 , 208 , 225 , 242 , 259 , 276 , 292 , 309 , 326 ,
342 , 358 , 375 , 391 , 407 , 423 , 438 , 454 , 469 , 485 ,
500 , 515 , 530 , 545 , 559 , 574 , 588 , 602 , 616 , 629 ,
643 , 656 , 669 , 682 , 695 , 707 , 719 , 731 , 743 , 755 ,
766 , 777 , 788 , 799 , 809 , 819 , 829 , 839 , 848 , 857 ,
866 , 875 , 883 , 891 , 899 , 906 , 914 , 921 , 927 , 934 ,
940 , 946 , 951 , 956 , 961 , 966 , 970 , 974 , 978 , 982 ,
985 , 988 , 990 , 993 , 995 , 996 , 998 , 999 , 999 , 1000 ,
1000 ,

: sin1000 ( deg -- sinx1000 )
    dup 180 < if
        dup 90 > invert if
            sintab + @                    \ deg in [0,90]
        else
            180 swap - sintab + @         \ deg in (90,180): sin(deg) = sin(180-deg)
        then
    else
        dup 270 > invert if
            180 - sintab + @ negate       \ deg in [180,270]: sin(deg) = -sin(deg-180)
        else
            360 swap - sintab + @ negate  \ deg in (270,360): sin(deg) = -sin(360-deg)
        then
    then
;
: cos1000 ( deg -- cosx1000 ) 90 + 360 mod sin1000 ;

\ --- Colour: a channel oscillates as 128 +/- 127*sin(hue + phase-offset) ---
: chan ( hue offset -- 0..255 ) + 360 mod sin1000 127 * 1000 / 128 + ;

variable win   variable ren
variable t     variable done
variable ang   variable hue
variable ct    variable st
variable r1  variable g1  variable b1     \ grating A colour (vertical lines)
variable r2  variable g2  variable b2     \ grating B colour (rotated lines), 180 deg out of phase

: set-colors ( hue -- )
    dup 0   chan r1 !
    dup 120 chan g1 !
    dup 240 chan b1 !
    dup 180 chan r2 !
    dup 300 chan g2 !
        420 chan b2 !
;

\ --- Input: drain all pending SDL events, quitting on QUIT or any keypress ---
: quit-or-key? ( etype -- flag ) dup QUIT_EVT = swap KEY_EVT = or ;

: poll-events ( -- )
    begin
        0 0 0 0 0 0 0 0 0 0 0 0 0 0 pollevent   ( f0 f1 .. f13 got )
        dup 0= if
            >r drop drop drop drop drop drop drop drop drop drop drop drop drop drop r> drop
            exit                                  \ no more queued events this frame
        then
        14 pick quit-or-key? if true done ! then  \ f0 is the event type
        >r drop drop drop drop drop drop drop drop drop drop drop drop drop drop r> drop
    again
;

\ --- Per-frame state: rotation angle and both gratings' colours ---
: update-frame ( -- )
    t @ 360 mod ang !
    t @ 3 / 360 mod hue !            \ colour cycles slower than the rotation
    ang @ cos1000 ct !
    ang @ sin1000 st !
    hue @ set-colors
;

variable dd  variable x1v  variable y1v  variable x2v  variable y2v

\ --- Grating A: fixed vertical lines (colour cycling) ---
: grating-a ( -- )
    ren @ r1 @ g1 @ b1 @ 110 sdlcolor drop
    101 NMIN do
        CX i SP * + x1v !
        ren @ x1v @ CY HALFLN - x1v @ CY HALFLN + sdlline drop
    loop
;

\ --- Grating B: the same lines, rotated by ANG about the centre (colour cycling, out of phase) ---
: grating-b ( -- )
    ren @ r2 @ g2 @ b2 @ 110 sdlcolor drop
    101 NMIN do
        i SP * dd !
        CX dd @ ct @ * 1000 / + HALFLN st @ * 1000 / + x1v !
        CY dd @ st @ * 1000 / + HALFLN ct @ * 1000 / - y1v !
        CX dd @ ct @ * 1000 / + HALFLN st @ * 1000 / - x2v !
        CY dd @ st @ * 1000 / + HALFLN ct @ * 1000 / + y2v !
        ren @ x1v @ y1v @ x2v @ y2v @ sdlline drop
    loop
;

: draw-frame ( -- )
    ren @ 0 0 0 255 sdlcolor drop
    ren @ sdlrclr drop
    grating-a
    grating-b
    ren @ sdlrpres
;

\ --- Initialise SDL ---
0 t !
0 done !
SDL_INIT_VIDEO sdlinit
0 < if ." SDL_Init failed" cr bye then

zstring" Moire Pattern - forth/dyncall/SDL2"
SDL_WINDOWPOS_CENTERED SDL_WINDOWPOS_CENTERED W H 4 sdlcwin win !
win @ 0= if ." SDL_CreateWindow failed" cr bye then

win @ -1 SDL_RENDERER_ACCELERATED sdlcrend ren !
ren @ 0= if ." SDL_CreateRenderer failed" cr bye then

ren @ SDL_BLENDMODE_BLEND sdlblend drop

." Animated moire pattern running - press any key or close the window to quit." cr

\ --- Animation loop: one iteration per frame ---
begin
    poll-events
    done @ 0= if
        update-frame
        draw-frame
        16 sdldelay
        t @ 1+ t !
    then
    done @
until

." Closing..." cr
ren @ sdldrend
win @ sdldwin
sdlquit
." Done." cr
