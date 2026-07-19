'use client';

import React, { useId, useMemo, useState } from 'react';
import {
  DndContext,
  PointerSensor,
  KeyboardSensor,
  useSensor,
  useSensors,
  closestCenter,
  type DragEndEvent,
} from '@dnd-kit/core';
import {
  SortableContext,
  arrayMove,
  rectSortingStrategy,
  sortableKeyboardCoordinates,
  useSortable,
} from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { Tile } from './Tile';
import { type TileAccess } from '../tileAccess';
import type { TileWithMeta } from '../tile-config';
import { saveGroupOrder } from '@/tileOrder';

interface SortableTileGridProps {
  group: string;
  items: Array<{ tile: TileWithMeta; access: TileAccess }>;
  activeTileId: string;
  onSelectTile: (t: any) => void;
  actorId?: number;
  targetLabel: (t: any) => string;
  currentUser: any;
  disabled?: boolean;
  onOrderChange?: (group: string, nextIds: string[]) => void;
}

function SortableTile({
  tile,
  access,
  active,
  onSelect,
  actorId,
  targetLabel,
  disabled,
}: {
  tile: TileWithMeta;
  access: TileAccess;
  active: boolean;
  onSelect: (t: any) => void;
  actorId?: number;
  targetLabel: (t: any) => string;
  disabled?: boolean;
}) {
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id: tile.id, disabled });

  const style: React.CSSProperties = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.55 : 1,
    zIndex: isDragging ? 10 : undefined,
    touchAction: disabled ? 'auto' : 'none',
  };

  return (
    <div
      ref={setNodeRef}
      style={style}
      {...(disabled ? {} : attributes)}
      {...(disabled ? {} : listeners)}
      className="h-full w-full min-w-0"
    >
      <Tile
        tile={tile}
        active={active}
        href={tile.href}
        state={access.state}
        reason={access.reason}
        requiredRoles={undefined}
        onClick={() => onSelect(tile)}
        actorId={actorId}
        targetLabel={targetLabel(tile)}
      />
    </div>
  );
}

export const SortableTileGrid: React.FC<SortableTileGridProps> = ({
  group,
  items,
  activeTileId,
  onSelectTile,
  actorId,
  targetLabel,
  currentUser,
  disabled = false,
  onOrderChange,
}) => {
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  const dndId = useId();

  const [local, setLocal] = useState<Array<{ tile: TileWithMeta; access: TileAccess }> | null>(null);
  const list = local ?? items;

  const ids = useMemo(() => list.map((i) => i.tile.id), [list]);

  const handleDragEnd = (event: DragEndEvent) => {
    if (disabled) return;
    const { active, over } = event;
    if (!over || active.id === over.id) return;
    const oldIndex = ids.indexOf(String(active.id));
    const newIndex = ids.indexOf(String(over.id));
    if (oldIndex < 0 || newIndex < 0) return;
    const next = arrayMove(list, oldIndex, newIndex);
    setLocal(next);
    const userId = currentUser?.id ?? -1;
    const orderedIds = next.map((i) => i.tile.id);
    saveGroupOrder(userId, group, orderedIds);
    onOrderChange?.(group, orderedIds);
  };

  return (
    <DndContext id={dndId} sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
      <SortableContext items={ids} strategy={rectSortingStrategy}>
        <div className="grid auto-rows-[190px] grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4">
          {list.map(({ tile, access }, idx) => (
            <div
              key={tile.id}
              className="animate-tile-rise min-w-0"
              style={{ animationDelay: `${Math.min(idx, 8) * 40}ms` }}
            >
              <SortableTile
                tile={tile}
                access={access}
                active={activeTileId === tile.id}
                onSelect={onSelectTile}
                actorId={actorId}
                targetLabel={targetLabel}
                disabled={disabled}
              />
            </div>
          ))}
        </div>
      </SortableContext>
    </DndContext>
  );
};

export default SortableTileGrid;
