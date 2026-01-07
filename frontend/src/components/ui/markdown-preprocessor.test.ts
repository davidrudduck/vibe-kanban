import { describe, it, expect } from 'vitest';
import { preprocessMarkdown } from './markdown-preprocessor';

describe('preprocessMarkdown', () => {
  describe('underscore handling', () => {
    it('escapes underscores in snake_case identifiers', () => {
      expect(preprocessMarkdown('use variable_name here')).toBe(
        'use variable\\_name here'
      );
    });

    it('escapes all underscores in VK_DATABASE_PATH', () => {
      // Both underscores should be escaped to prevent _DATABASE_ becoming italic
      expect(preprocessMarkdown('VK_DATABASE_PATH')).toBe(
        'VK\\_DATABASE\\_PATH'
      );
    });

    it('preserves leading underscores (only right side is word char)', () => {
      // _DATABASE starts with underscore but has no word char on left
      // Only the middle underscore (between DATABASE and PATH) gets escaped
      expect(preprocessMarkdown('_DATABASE_PATH')).toBe('_DATABASE\\_PATH');
    });

    it('escapes multi-underscore identifiers', () => {
      expect(preprocessMarkdown('one_two_three_four')).toBe(
        'one\\_two\\_three\\_four'
      );
    });

    it('preserves intentional italic with spaces around both underscores', () => {
      // Intentional italic: space before opening underscore, space after closing
      // Neither underscore is escaped because neither has word chars on BOTH sides
      expect(preprocessMarkdown('this is _italic_ text')).toBe(
        'this is _italic_ text'
      );
    });

    it('handles mixed intentional italic and snake_case', () => {
      // _important_ preserved (spaces around), variable_name escaped (word chars both sides)
      expect(preprocessMarkdown('use _important_ variable_name')).toBe(
        'use _important_ variable\\_name'
      );
    });

    it('preserves underscores inside backticks', () => {
      expect(preprocessMarkdown('use `snake_case` here')).toBe(
        'use `snake_case` here'
      );
    });

    it('preserves underscores in fenced code blocks', () => {
      const input = '```\nsnake_case_var\n```';
      expect(preprocessMarkdown(input)).toBe(input);
    });

    it('preserves underscores in URLs', () => {
      expect(preprocessMarkdown('see https://example.com/foo_bar')).toBe(
        'see https://example.com/foo_bar'
      );
    });

    it('handles empty string', () => {
      expect(preprocessMarkdown('')).toBe('');
    });

    it('preserves underscore at end of word only (trailing)', () => {
      // Trailing underscore: word char on left, nothing on right - not escaped
      expect(preprocessMarkdown('test_')).toBe('test_');
    });

    it('preserves underscore at start of word only (leading)', () => {
      // Leading underscore: nothing on left, word char on right - not escaped
      expect(preprocessMarkdown('_test')).toBe('_test');
    });

    it('preserves standalone underscore surrounded by spaces', () => {
      expect(preprocessMarkdown('a _ b')).toBe('a _ b');
    });
  });
});
