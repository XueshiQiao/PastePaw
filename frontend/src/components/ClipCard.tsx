import { ClipboardItem } from '../types';
import { clsx } from 'clsx';
import { useMemo, memo, useState, forwardRef } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { PREVIEW_CHAR_LIMIT } from '../constants';
import { Copy, Check } from 'lucide-react';
import { useMotionValue, useMotionTemplate, motion } from 'framer-motion';

interface ClipCardProps {
  clip: ClipboardItem;
  isSelected: boolean;
  layout: { columnWidth: number; cardGap: number; cardVerticalPadding: number };
  onSelect: () => void;
  onPaste: () => void;
  onCopy: () => void;
  onDragStart: (clipId: string, startX: number, startY: number) => void;
  onContextMenu?: (e: React.MouseEvent) => void;
}

export const ClipCard = memo(
  forwardRef<HTMLDivElement, ClipCardProps>(function ClipCard(
    { clip, isSelected, layout, onSelect, onPaste, onCopy, onDragStart, onContextMenu }: ClipCardProps,
    ref
  ) {
    const { t } = useTranslation();
    const [copied, setCopied] = useState(false);
    const [hovered, setHovered] = useState(false);
    const title = clip.source_app || clip.clip_type.toUpperCase();

    const mouseX = useMotionValue(0);
    const mouseY = useMotionValue(0);

    const imageSrc = useMemo(() => {
      if (clip.clip_type !== 'image' || !clip.content) return null;
      const value = clip.content;
      const isAbsolutePath = value.startsWith('/') || /^[A-Za-z]:[\\/]/.test(value);
      if (
        value.startsWith('data:') ||
        value.startsWith('http://') ||
        value.startsWith('https://') ||
        value.startsWith('asset:') ||
        value.startsWith('tauri://')
      ) {
        return value;
      }
      if (isAbsolutePath) {
        return convertFileSrc(value);
      }
      return `data:image/png;base64,${value}`;
    }, [clip.clip_type, clip.content]);

    const imageSizeKb = useMemo(() => {
      if (clip.clip_type !== 'image') return 0;
      try {
        const parsed = clip.metadata
          ? (JSON.parse(clip.metadata) as { size_bytes?: number })
          : null;
        if (parsed?.size_bytes && parsed.size_bytes > 0) {
          return Math.round(parsed.size_bytes / 1024);
        }
      } catch {
        // Ignore invalid metadata and fall back to zero.
      }
      return 0;
    }, [clip.clip_type, clip.metadata]);

    // Memoize the content rendering
    const renderedContent = useMemo(() => {
      if (clip.clip_type === 'image') {
        return (
          <div className="flex h-full w-full select-none items-center justify-center">
            {clip.content ? (
              <img
                src={imageSrc ?? undefined}
                alt="Clipboard Image"
                className="max-h-full max-w-full object-contain"
              />
            ) : (
              <span className="text-xs text-muted-foreground/70">Image</span>
            )}
          </div>
        );
      } else {
        return (
          <pre className="whitespace-pre-wrap break-all font-mono text-[13px] leading-tight text-foreground">
            <span>{clip.content.substring(0, PREVIEW_CHAR_LIMIT)}</span>
          </pre>
        );
      }
    }, [clip.clip_type, clip.content, imageSrc]);

    // Generate stable color index based on source app name
    const getAppColorIndex = (name: string) => {
      let hash = 0;
      for (let i = 0; i < name.length; i++) {
        hash = name.charCodeAt(i) + ((hash << 5) - hash);
      }
      return Math.abs(hash) % 15;
    };

    const appHue = useMemo(() => {
      const index = getAppColorIndex(title);
      const hueStep = 360 / 15;
      return Math.round(index * hueStep);
    }, [title]);

    const glowBackground = useMotionTemplate`radial-gradient(180px circle at ${mouseX}px ${mouseY}px, hsl(${appHue} 90% 64% / 0.9), transparent 65%)`;

    const handleMouseDown = (e: React.MouseEvent) => {
      // Only left click
      if (e.button !== 0) return;
      onDragStart(clip.id, e.clientX, e.clientY);
    };

    const handleContextMenu = (e: React.MouseEvent) => {
      e.preventDefault();
      onContextMenu?.(e);
    };

    const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
      const rect = e.currentTarget.getBoundingClientRect();
      mouseX.set(e.clientX - rect.left);
      mouseY.set(e.clientY - rect.top);
    };

    return (
      <div
        ref={ref}
        data-el="clip-card"
        data-clip-id={clip.id}
        style={{
          width: layout.columnWidth - layout.cardGap,
          height: `calc(100% - ${layout.cardVerticalPadding * 2}px)`,
        }}
        className="flex-shrink-0"
      >
        <div
          data-el="clip-card-inner"
          onMouseDown={handleMouseDown}
          onMouseMove={handleMouseMove}
          onMouseEnter={() => setHovered(true)}
          onMouseLeave={() => setHovered(false)}
          onClick={onSelect}
          onDoubleClick={onPaste}
          onContextMenu={handleContextMenu}
          style={
            {
              '--app-hue': `${appHue}`,
              borderColor: isSelected ? `hsl(${appHue} 82% 60%)` : undefined,
              boxShadow: isSelected ? `inset 0 0 0 1px hsl(${appHue} 82% 60%)` : undefined,
            } as React.CSSProperties
          }
          className={clsx(
            'relative flex h-full w-full cursor-pointer select-none flex-col overflow-hidden rounded-2xl border border-border bg-card/80 transition-all',
            isSelected ? 'z-10 scale-[1.02] transform shadow-xl' : 'hover:-translate-y-1 shadow-lg',
            'group'
          )}
        >
          {/* Framer-motion spotlight border glow */}
          {!isSelected && (
            <motion.div
              data-el="clip-card-glow"
              className="pointer-events-none absolute -inset-px z-20 rounded-[17px] p-[2px]"
              style={{
                background: glowBackground,
                WebkitMask: 'linear-gradient(#000 0 0) content-box, linear-gradient(#000 0 0)',
                WebkitMaskComposite: 'xor',
                maskComposite: 'exclude',
                opacity: hovered ? 1 : 0,
                transition: 'opacity 200ms',
              }}
            />
          )}

          <div
            data-el="clip-card-header"
            className="relative z-10 flex flex-shrink-0 items-center gap-2 px-2 py-1.5"
            style={{ backgroundColor: `hsl(${appHue} 82% 60%)` }}
          >
            {clip.source_icon && (
              <img
                src={`data:image/png;base64,${clip.source_icon}`}
                alt=""
                className="h-4 w-4 object-contain"
              />
            )}
            <span className="flex-1 truncate text-[11px] font-bold uppercase tracking-wider text-black/80">
              {title}
            </span>
            <button
              data-el="clip-card-copy-btn"
              onClick={(e) => {
                e.stopPropagation();
                onCopy();
                setCopied(true);
                setTimeout(() => setCopied(false), 2000);
              }}
              className="rounded-md p-1 opacity-0 transition-all hover:bg-black/10 group-hover:opacity-100"
              title="Copy to clipboard"
            >
              {copied ? (
                <Check size={14} className="text-emerald-500" />
              ) : (
                <Copy size={14} className="text-foreground/70 hover:text-foreground" />
              )}
            </button>
          </div>

          <div
            data-el="clip-card-content"
            className="relative z-10 flex-1 overflow-hidden bg-card/90 p-2"
          >
            {renderedContent}
            <div className="pointer-events-none absolute bottom-0 left-0 right-0 h-12 bg-gradient-to-t from-card/100 to-card/30" />
          </div>

          <div
            data-el="clip-card-footer"
            className="absolute bottom-0 left-0 right-0 z-10 bg-gradient-to-t from-card via-card/100 to-transparent/0 px-3 py-1.5"
          >
            <span className="text-[11px] font-medium text-muted-foreground/50">
              {clip.clip_type === 'image'
                ? t('clipList.imageSize', { size: imageSizeKb })
                : t('clipList.textLength', { count: clip.content.length })}
            </span>
          </div>
        </div>
      </div>
    );
  })
);
