import { Test } from '__ENTRY__';
const test = new Test();
const letter: 'a' = test.letter;
// @ts-expect-error the initializer retains the enum-member literal type
const wrongLetter: 'b' = test.letter;
// @ts-expect-error the property remains readonly
test.letter = test.letter;
// The source checker exposes ambient Alphabet implicitly; visibility is recorded in the review.
