when creating a user we should have an icon to show password

when creating a user we should allow creating as admin checkbox

convert plaintext passwords to hashes

api for games, moves

conversations, messages, api and schema





multiple kinds of gameplay:
- normal chess
- if you have a move that ends with you taking a piece, you must take such a move
	- and the king is just a normal piece, there is no checkmate, you win if you lose all your pieces
	- and you're still trying to do a normal sort of checkmate, you win if you checkmate the other guy

AI and human players

client-server, initial version will be web

websockets for real time updates

a chat functionality
messages saved on a game, with sender and timestamp
observers can be invited to a game and send and receive messages
