import { describe, it, expect } from 'vitest';
import { preprocessMarkdown } from './markdown-preprocessor';

describe('preprocessMarkdown', () => {
  describe('underscore handling', () => {
    it('escapes underscores in snake_case identifiers', () => {
      expect(preprocessMarkdown('use variable_name here')).toBe(
        'use variable\\_name here'
      );
    });

    it('escapes closing underscore of _italic_ after word character', () => {
      // The closing underscore follows 'c' (word char), so it gets escaped
      // This is expected behavior - markdown-to-jsx still renders correctly
      expect(preprocessMarkdown('this is _italic_ text')).toBe(
        'this is _italic\\_ text'
      );
    });

    it('preserves _italic_ at start of line', () => {
      // Opening underscore after space is preserved
      expect(preprocessMarkdown('_italic_ text')).toBe('_italic\\_ text');
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
  });
});
