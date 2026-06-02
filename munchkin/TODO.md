refactoring:
- drop useless ocr fields
  - title_match_score
  - bbox
  - raw_line
  - title_raw
- rename, and document the field ordering:
  - above_title
  - title
  - below_title
  - body
- all fields that contain text can be either strings or arrays of strings

double check cards.toml to make sure all monsters have
- a level (where? above_title?)
- treasures

markdown in various text fields? at least bold and italics


clean up the svg data to have transparent backgrounds


make a card renderer


commit card-image-scanner/out, maybe under a better path