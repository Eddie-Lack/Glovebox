Glovebox
========

A level editor for Glover (Nintendo 64, USA version)

It runs on your computer, in its own window


You need your own ROM!
----------------------

Glover (USA) as a .z64, .v64 or .n64 file


Starting it
-----------

You need 64-bit Windows 10 or 11

Double-click Glovebox.exe
  Windows may say "Windows protected your PC", because the app isn't signed
  Choose "More info", then "Run anyway"

The editor opens in its own window, in under a second
Close the window to stop it

Glovebox.exe is the whole app. It doesn't install or unpack anything

Start it again while it runs and it tells you it's already running


Graphics
--------

The editor draws the level with Direct3D 12, or with Vulkan
Any graphics card made since about 2013 does one of them

Without one, Windows draws it in software. That works, only slower
  That's what you get in Remote Desktop or a virtual machine without 3D

editor.log in the user folder says which graphics card the editor uses


The editor could not start its window!!
---------------------------------------

Update the driver from your graphics card's maker (NVIDIA, AMD or Intel)
In Remote Desktop or a virtual machine without 3D, start the editor on the computer itself


The first start
---------------

A setup screen asks for your ROM. Press "Choose the ROM…" and pick it

The app checks it's the right ROM and keeps a copy in its user folder

Then the editor opens, with the first level. There's nothing to wait for


The user folder
---------------

It holds the ROM copy, your autosaved edits (drafts), the editor's settings,
the ROMs Test level builds (test) and the models you import with Import model (custom)
It's %LOCALAPPDATA%\Glovebox. The setup screen shows its path

editor.log there holds the editor's messages. Send it with a bug report

Delete the user folder to remove everything the app wrote


Options
-------

  --home DIR     use another user folder (new or empty the first time)
  --reset        forget the ROM and ask for it again
  --fixture      made-up levels instead of your ROM's, to try the editor without a ROM
  --self-test    check that the app and your graphics work, then stop
  --version      say the version, then stop

Run these from a command prompt, for example: Glovebox.exe --reset

The app has no console, so --self-test and --version only show something when you send it to a file:
  Glovebox.exe --self-test > report.txt


Licences
--------

THIRD_PARTY_NOTICES.md names the projects the editor builds on

The licenses folder holds the licences of the Rust libraries and the fonts inside the app, each in a folder named after the library
