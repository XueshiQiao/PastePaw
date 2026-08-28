export const LAYOUT = {
  WINDOW_HEIGHT: 266, // keep sync with backend (constants.rs)
  CONTROL_BAR_HEIGHT: 50,
  CARD_WIDTH: 210,
  CARD_GAP: 16,
  SIDE_PADDING: 20,
  CARD_VERTICAL_PADDING: 8,
  PADDING_OPACITY: 0.2,
  WINDOW_PADDING: 8, // In pixels
  BLUR_AMOUNT: '8px', // Intensity of the blur
};

export const CLIP_LIST_HEIGHT = LAYOUT.WINDOW_HEIGHT - LAYOUT.CONTROL_BAR_HEIGHT;
// Width of each virtual grid column cell = card width + gap between cards
export const COLUMN_WIDTH = LAYOUT.CARD_WIDTH + LAYOUT.CARD_GAP;
export const PREVIEW_CHAR_LIMIT = 300;
