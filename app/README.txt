Glovebox Version 1.0.9
=====================

A level editor for Glover (Nintendo 64, USA version).

Guide: https://github.com/Eddie-Lack/Glovebox/tree/main/docs/guide


You need your own ROM! Glover (USA) as a .z64, .v64 or .n64 file
----------------------------------------------------------------


Starting it
-----------

Double-click Glovebox.exe.
  > Windows may say "Windows protected your PC", because the app isn't signed - Choose "More info", then "Run anyway".


On the first start, the setup screen asks for your ROM. Click "Choose the ROM…" and pick it.

The app verifies that it's the correct ROM, and keeps a copy in the user folder.


The user folder
---------------

Holds the ROM copy, your autosaved edits (drafts), the editor's settings, the ROMs that Test level builds (test) and the models you import with Import model (custom)
It's located at %LOCALAPPDATA%\Glovebox.
editor.log there holds the editor's messages, please send it with any bug reports

Delete the user folder to remove everything the app wrote.


Options
-------

  --home DIR     use another user folder (new or empty the first time)
  --reset        forget the ROM and ask for it again
  --fixture      made-up levels instead of your ROM's, to try the editor without a ROM
  --self-test    check that the app and your graphics work
  --version      print the version

Run these from a command prompt, for example: Glovebox.exe --reset

The app has no console, so --self-test and --version only show something when you send it to a file:
  Glovebox.exe --self-test > report.txt

The editor could not start its window!!
---------------------------------------

Update the driver from your graphics card's maker (NVIDIA, AMD or Intel).


Licenses
--------

see THIRD_PARTY_NOTICES.md and the licenses folder
