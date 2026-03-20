\ Sound demo using mciSendStringA via dyncall
\ mciSendStringA(cmd: cstr, retbuf: ptr, retbuflen: u32, hwnd: ptr) -> u32

extern: mci "winmm.dll|mciSendStringA|cstr,ptr,u32,ptr|u32|coerce"

\ Helper: send an MCI command and drop the return value
: mci-cmd ( str -- )
    0 0 0 mci drop ;

." Playing 4 sounds..." cr

zstring" open C:/Windows/Media/tada.wav type waveaudio alias snd" mci-cmd
zstring" play snd wait" mci-cmd
zstring" close snd" mci-cmd

zstring" open C:/Windows/Media/chimes.wav type waveaudio alias snd" mci-cmd
zstring" play snd wait" mci-cmd
zstring" close snd" mci-cmd

zstring" open C:/Windows/Media/chord.wav type waveaudio alias snd" mci-cmd
zstring" play snd wait" mci-cmd
zstring" close snd" mci-cmd

zstring" open C:/Windows/Media/ding.wav type waveaudio alias snd" mci-cmd
zstring" play snd wait" mci-cmd
zstring" close snd" mci-cmd

." Done!" cr
