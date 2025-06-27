import React from "react";

interface TitlebarButtonProps {
  icon: string;
  alt: string;
  onClick: (e: React.MouseEvent) => void;
  id?: string; // Optional ID prop
}

export const TitlebarButton: React.FC<TitlebarButtonProps> = ({
  icon,
  alt,
  onClick,
  id,
}) => {
  return (
    <div
      id={id}
      className="titlebar-button p-2 hover:bg-gray-700 cursor-pointer transition-colors duration-200"
      onClick={(e) => {
        e.stopPropagation(); // Always prevent propagation
        console.log(
          `TitlebarButton clicked: ${alt}${id ? ` (id: ${id})` : ""}`
        );
        onClick(e);
      }}
    >
      <img
        src={icon}
        alt={alt}
        className="w-4 h-4 filter invert" // Consistent sizing and color
      />
    </div>
  );
};
