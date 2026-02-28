- s3 mount type

- google drive mount type

- sync

- web ui: should have a previous if the mime type is text or image

- web ui: should last modified time in table

- web ui: should have a text editor

- web ui: should let me download whole directories, zip first

- copy/cut/paste directories
	- web ui
	- cli

- progress bars: server
	- server needs to be able to track progress of operations
	- api to get list of pending operations
	- real time updates for operation progress and completion

- progress bars: cli: 
	- new command to show pending operations
	- existing commands should wait until their operation completes and show a progress bar

- progress bars: web ui:
	- show overall status in the bottom right of the screen
	- new top-level tab "queue" that shows progress bars for all current operations
	- when starting a new operation show a modal with a progress bar, which if you click on it dismisses that and lets you keep working
	- whenever a real time update shows up indicating an operation is completed both sides of the page should refresh

- web ui: themes, customize css colors

- web ui: right click on the background of a column, new actions:
	- upload a file
	- make a directory
