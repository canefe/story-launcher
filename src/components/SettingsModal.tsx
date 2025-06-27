import { useState, useEffect } from "react";
import { useSettingsStore } from "../store/settings";

interface SettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export function SettingsModal({ isOpen, onClose }: SettingsModalProps) {
  const { instanceFolderName, setInstanceFolderName } = useSettingsStore();
  const [folderName, setFolderName] = useState(instanceFolderName);

  // Reset form state when modal opens
  useEffect(() => {
    if (isOpen) {
      setFolderName(instanceFolderName);
    }
  }, [isOpen, instanceFolderName]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    setInstanceFolderName(folderName);
    onClose();
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
      <div className="bg-gray-800 p-6 rounded-lg w-96 max-w-full">
        <h2 className="text-xl font-bold mb-4">Settings</h2>

        <form onSubmit={handleSubmit}>
          <div className="mb-4">
            <label htmlFor="folderName" className="block mb-2">
              Instance Folder Name
            </label>
            <input
              id="folderName"
              type="text"
              className="w-full p-2 bg-gray-700 rounded"
              value={folderName}
              onChange={(e) => setFolderName(e.target.value)}
              placeholder="Instance folder name"
            />
            <p className="text-xs text-gray-400 mt-1">
              This is the folder name used for the Minecraft instance.
            </p>
          </div>

          <div className="flex justify-end gap-2">
            <button
              type="button"
              className="px-4 py-2 bg-gray-600 hover:bg-gray-500 rounded"
              onClick={onClose}
            >
              Cancel
            </button>
            <button
              type="submit"
              className="px-4 py-2 bg-blue-600 hover:bg-blue-500 rounded"
            >
              Save
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
