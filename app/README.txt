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
Your graphics card must do OpenGL 3.3 (any made since about 2010)

Double-click Glovebox.exe
  Windows may say "Windows protected your PC", because the app isn't signed
  Choose "More info", then "Run anyway"

The editor opens in its own window
Close the window to stop it

It takes a few seconds to start. It unpacks itself into a temporary folder each time

Start it again while it runs and it tells you it's already running


It says OpenGL is missing!!
---------------------------

The editor draws the level with OpenGL 3.3, through your graphics driver

Update the driver from your graphics card's maker (NVIDIA, AMD or Intel)
"GDI Generic" or "Microsoft Basic Render Driver" in the message means Windows has no driver for the card
In Remote Desktop or a virtual machine without 3D, start the editor on the computer itself


The first start
---------------

A setup window asks for your ROM. Press "Choose the ROM…" and pick it

The app checks it's the right ROM, keeps a copy in its user folder and prepares the levels

Then the editor takes a few minutes to start, while it converts the game's models
Later starts take seconds


The user folder
---------------

It holds the ROM copy, everything the app makes from it, your autosaved edits (drafts), the editor's settings,
the ROMs Test level builds (test) and the models you import with Import model (custom)
It's %LOCALAPPDATA%\Glovebox. The setup window shows its path

editor.log there holds the editor's messages. Send it with a bug report

Delete the user folder to remove everything the app wrote


Options
-------

  --home DIR     use another user folder (new or empty the first time)
  --reset        forget the ROM and ask for it again
  --self-test    check that the app has everything it needs, then stop

Run these from a command prompt, for example: Glovebox.exe --reset

The app has no console, so --self-test only shows its report when you send it to a file:
  Glovebox.exe --self-test > report.txt


Licences
--------

The licenses folder holds the licences of Python and of the libraries inside the app, each in a folder named after the library

The Unicorn and Keystone engines are under the GNU General Public License, version 2 (unicorn-COPYING and keystone-COPYING)
Their source code is attached to each release of the app, as unicorn-*.tar.gz and keystone-engine-*.tar.gz
