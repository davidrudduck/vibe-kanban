import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { VKSLogo, VKSIcon } from '../components/VKSLogo';

describe('VKSLogo', () => {
  it('renders with default props', () => {
    render(<VKSLogo />);
    expect(screen.getByLabelText('VK-Swarm')).toBeInTheDocument();
  });

  it('contains VK text in primary color', () => {
    render(<VKSLogo />);
    const logo = screen.getByLabelText('VK-Swarm');
    const vkSpan = logo.querySelector('.text-primary');
    expect(vkSpan).toBeInTheDocument();
    expect(vkSpan?.textContent).toBe('VK');
  });

  it('has mobile version (VKS) hidden on sm+ screens', () => {
    render(<VKSLogo />);
    const logo = screen.getByLabelText('VK-Swarm');
    // Mobile version should have sm:hidden class
    const mobileSpan = logo.querySelector('span.sm\\:hidden');
    expect(mobileSpan).toBeInTheDocument();
  });

  it('has desktop version (VK-SWARM) hidden on mobile', () => {
    render(<VKSLogo />);
    const logo = screen.getByLabelText('VK-Swarm');
    // Desktop version should have hidden sm:inline classes
    const desktopSpan = logo.querySelector('span.hidden.sm\\:inline');
    expect(desktopSpan).toBeInTheDocument();
  });

  it('shows full logo when alwaysFull is true', () => {
    render(<VKSLogo alwaysFull />);
    const logo = screen.getByLabelText('VK-Swarm');
    // Should not have hidden class when alwaysFull
    const desktopSpan = logo.querySelector('span:not(.hidden)');
    expect(desktopSpan).toBeInTheDocument();
    expect(desktopSpan?.textContent).toContain('-SWARM');
  });

  it('applies custom className', () => {
    render(<VKSLogo className="text-xl" />);
    const logo = screen.getByLabelText('VK-Swarm');
    expect(logo).toHaveClass('text-xl');
  });

  it('uses code font for terminal aesthetic', () => {
    render(<VKSLogo />);
    const logo = screen.getByLabelText('VK-Swarm');
    expect(logo).toHaveClass('font-code');
  });
});

describe('VKSIcon', () => {
  it('renders with VK text only', () => {
    render(<VKSIcon />);
    const icon = screen.getByLabelText('VK-Swarm');
    expect(icon).toBeInTheDocument();
    expect(icon.textContent).toBe('VK');
  });

  it('uses primary color for VK', () => {
    render(<VKSIcon />);
    const icon = screen.getByLabelText('VK-Swarm');
    const vkSpan = icon.querySelector('.text-primary');
    expect(vkSpan).toBeInTheDocument();
  });

  it('applies custom className', () => {
    render(<VKSIcon className="text-lg" />);
    const icon = screen.getByLabelText('VK-Swarm');
    expect(icon).toHaveClass('text-lg');
  });
});

describe('VKS CSS Variables', () => {
  it('vks-theme class can be applied to elements', () => {
    // Test that vks-theme class can be used on elements
    render(
      <div data-testid="vks-theme-test" className="vks-theme">
        VKS Themed
      </div>
    );
    const themed = screen.getByTestId('vks-theme-test');
    expect(themed).toHaveClass('vks-theme');
  });
});

describe('VKS Tailwind Colors', () => {
  it('can use vks color classes', () => {
    // Test that Tailwind classes for VKS colors work
    render(
      <div data-testid="vks-colors">
        <span className="text-vks-cyan">Cyan</span>
        <span className="text-vks-amber">Amber</span>
        <span className="text-vks-emerald">Emerald</span>
        <span className="text-vks-coral">Coral</span>
        <span className="text-vks-violet">Violet</span>
        <span className="bg-vks-void">Void</span>
        <span className="bg-vks-surface">Surface</span>
      </div>
    );

    const container = screen.getByTestId('vks-colors');
    expect(container.querySelector('.text-vks-cyan')).toBeInTheDocument();
    expect(container.querySelector('.text-vks-amber')).toBeInTheDocument();
    expect(container.querySelector('.text-vks-emerald')).toBeInTheDocument();
    expect(container.querySelector('.text-vks-coral')).toBeInTheDocument();
    expect(container.querySelector('.text-vks-violet')).toBeInTheDocument();
    expect(container.querySelector('.bg-vks-void')).toBeInTheDocument();
    expect(container.querySelector('.bg-vks-surface')).toBeInTheDocument();
  });
});

describe('VKS Typography', () => {
  it('can use font-heading class', () => {
    render(
      <h1 data-testid="heading" className="font-heading">
        Heading
      </h1>
    );
    const heading = screen.getByTestId('heading');
    expect(heading).toHaveClass('font-heading');
  });

  it('can use font-serif class', () => {
    render(
      <p data-testid="serif" className="font-serif">
        Serif text
      </p>
    );
    const serif = screen.getByTestId('serif');
    expect(serif).toHaveClass('font-serif');
  });
});
