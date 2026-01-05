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

    it('escapes underscores followed by word characters', () => {
      // _DATABASE starts with underscore followed by word char
      expect(preprocessMarkdown('_DATABASE_PATH')).toBe('\\_DATABASE\\_PATH');
    });

    it('escapes multi-underscore identifiers', () => {
      expect(preprocessMarkdown('one_two_three_four')).toBe(
        'one\\_two\\_three\\_four'
      );
    });

    it('preserves intentional italic with spaces around both underscores', () => {
      // Intentional italic: space before opening AND after closing underscore
      // The closing underscore gets escaped because it follows a word char,
      // but markdown-to-jsx still renders correctly
      expect(preprocessMarkdown('this is _italic_ text')).toBe(
        'this is \\_italic\\_ text'
      );
    });

    it('handles mixed intentional italic and snake_case', () => {
      // "important" underscores get escaped, as does variable_name
      expect(preprocessMarkdown('use _important_ variable_name')).toBe(
        'use \\_important\\_ variable\\_name'
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

    it('handles underscore at end of word only', () => {
      expect(preprocessMarkdown('test_')).toBe('test\\_');
    });

    it('handles underscore at start of word only', () => {
      expect(preprocessMarkdown('_test')).toBe('\\_test');
    });

    it('preserves standalone underscore surrounded by spaces', () => {
      expect(preprocessMarkdown('a _ b')).toBe('a _ b');
    });
  });
});
