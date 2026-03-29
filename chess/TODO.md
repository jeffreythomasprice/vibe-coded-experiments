dev.sh still doesn't reliably kill server on exit



Anybody who can see a game (participant, or admin) should be able to observe the current state of the game. This places the table with a canvas that displays the current board state.

For now placeholder graphics that just put a colored circle in the square with a single text character for the piece type can be used.
P = pawn
N = knight
B = bishop
R = rook
Q = queen
K = king

We don't need to implement making moves yet. We'll expand this canvas and the API to actually play the games later.



when making a new game we should auto-complete username text box

convert plaintext passwords to hashes

multiple kinds of gameplay:
- normal chess
- if you have a move that ends with you taking a piece, you must take such a move
	- and the king is just a normal piece, there is no checkmate, you win if you lose all your pieces
	- and you're still trying to do a normal sort of checkmate, you win if you checkmate the other guy

websockets for real time updates

a chat functionality
messages saved on a game, with sender and timestamp
observers can be invited to a game and send and receive messages

AI moves, initial AI can just be random moves
