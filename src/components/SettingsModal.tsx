import { useState, useEffect } from "react";
import { useSettingsStore } from "../store/settings";
import { invoke } from "@tauri-apps/api/core";
import { homeDir } from "@tauri-apps/api/path";

interface SettingsModalProps {
	isOpen: boolean;
	onClose: () => void;
}

export function SettingsModal({ isOpen, onClose }: SettingsModalProps) {
	const {
		instanceFolderName,
		setInstanceFolderName,
		instancesPath,
		setInstancesPath,
		availableLaunchers,
		setAvailableLaunchers,
		selectedLauncher,
		setSelectedLauncher,
	} = useSettingsStore();
	const [folderName, setFolderName] = useState(instanceFolderName);
	const [isSearching, setIsSearching] = useState(false);
	const [searchResult, setSearchResult] = useState("");

	// Reset form state when modal opens
	useEffect(() => {
		if (isOpen) {
			setFolderName(instanceFolderName);
			setSearchResult("");
		}
	}, [isOpen, instanceFolderName]);

	const handleSubmit = (e: React.FormEvent) => {
		e.preventDefault();
		setInstanceFolderName(folderName);
		onClose();
	};

	const searchForLauncherPaths = async () => {
		setIsSearching(true);
		setSearchResult("");
		
		try {
			const home = await homeDir();
			const platform = navigator.platform.toLowerCase();
			
			// Define possible launchers with their paths and types
			const launcherCandidates = [];
			
			if (platform.includes('win')) {
				// Windows paths
				launcherCandidates.push(
					{ name: 'MultiMC', path: `${home}\\AppData\\Roaming\\MultiMC\\instances\\`, type: 'MultiMC' as const },
					{ name: 'PollyMC', path: `${home}\\AppData\\Roaming\\PollyMC\\instances\\`, type: 'PollyMC' as const },
					{ name: 'PrismLauncher', path: `${home}\\AppData\\Roaming\\PrismLauncher\\instances\\`, type: 'PrismLauncher' as const }
				);
			} else if (platform.includes('mac')) {
				// macOS paths
				launcherCandidates.push(
					{ name: 'MultiMC', path: `${home}/Library/Application Support/MultiMC/instances/`, type: 'MultiMC' as const },
					{ name: 'PollyMC', path: `${home}/Library/Application Support/PollyMC/instances/`, type: 'PollyMC' as const },
					{ name: 'PrismLauncher', path: `${home}/Library/Application Support/PrismLauncher/instances/`, type: 'PrismLauncher' as const }
				);
			} else {
				// Linux paths
				launcherCandidates.push(
					{ name: 'MultiMC', path: `${home}/.local/share/MultiMC/instances/`, type: 'MultiMC' as const },
					{ name: 'PollyMC', path: `${home}/.local/share/PollyMC/instances/`, type: 'PollyMC' as const },
					{ name: 'PrismLauncher', path: `${home}/.local/share/PrismLauncher/instances/`, type: 'PrismLauncher' as const }
				);
			}
			
			// Check which launchers exist
			const foundLaunchers = [];
			for (const launcher of launcherCandidates) {
				try {
					const result = await invoke("check_path_exists", { path: launcher.path });
					if (result) {
						console.log(`Found launcher: ${launcher.name} at ${launcher.path}`);
						// Store the full absolute path
						foundLaunchers.push({
							...launcher,
							path: launcher.path
						});
					}
				} catch (error) {
					console.log(`Launcher ${launcher.name} not found: ${error}`);
				}
			}
			
			// Update available launchers
			setAvailableLaunchers(foundLaunchers);
			
			if (foundLaunchers.length === 0) {
				setSearchResult("No launcher instances folder found. Please set manually.");
			} else if (foundLaunchers.length === 1) {
				// Auto-select if only one found
				const launcher = foundLaunchers[0];
				setSelectedLauncher(launcher);
				setInstancesPath(launcher.path);
				setSearchResult(`Found: ${launcher.name}`);
			} else {
				// Multiple launchers found - let user choose
				setSearchResult(`Found ${foundLaunchers.length} launchers. Please select one from the dropdown.`);
			}
		} catch (error) {
			setSearchResult(`Error searching for launchers: ${error}`);
		} finally {
			setIsSearching(false);
		}
	};

	if (!isOpen) return null;

	return (
		<div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
			<div className="bg-gray-800 p-6 w-96 max-w-full">
				<h2 className="text-xl font-bold mb-4">Settings</h2>

				<form onSubmit={handleSubmit}>
					<div className="mb-4">
						<label htmlFor="folderName" className="block mb-2">
							Instance Folder Name
						</label>
						<input
							id="folderName"
							type="text"
							className="w-full p-2 bg-gray-700"
							value={folderName}
							onChange={(e) => setFolderName(e.target.value)}
							placeholder="Instance folder name"
						/>
						<p className="text-xs text-gray-400 mt-1">
							This is the folder name used for the Minecraft
							instance.
						</p>
					</div>
					<div className="mb-4">
						<label htmlFor="instancesPath" className="block mb-2">
							Instances Path (MultiMC launchers)
						</label>
						<div className="flex flex-col gap-2">
							<input
								id="instancesPath"
								type="text"
								className="flex-1 p-2 bg-gray-700"
								value={instancesPath}
								onChange={(e) => setInstancesPath(e.target.value)}
								placeholder="Instances path"
							/>
							<button
								type="button"
								className="px-3 py-2 bg-green-600 hover:bg-green-500 cursor-pointer disabled:bg-gray-500 disabled:cursor-not-allowed"
								onClick={searchForLauncherPaths}
								disabled={isSearching}
							>
								{isSearching ? "Searching..." : "Auto-detect"}
							</button>
						</div>
						
						{/* Launcher Selection Dropdown */}
						{availableLaunchers.length > 1 && (
							<div className="mt-3">
								<label htmlFor="launcherSelect" className="block mb-2 text-sm">
									Select Launcher:
								</label>
								<select
									id="launcherSelect"
									className="w-full p-2 bg-gray-700"
									value={selectedLauncher?.name || ""}
									onChange={(e) => {
										const launcher = availableLaunchers.find(l => l.name === e.target.value);
										if (launcher) {
											setSelectedLauncher(launcher);
											setInstancesPath(launcher.path);
										}
									}}
								>
									<option value="">Choose a launcher...</option>
									{availableLaunchers.map((launcher) => (
										<option key={launcher.name} value={launcher.name}>
											{launcher.name} - {launcher.path}
										</option>
									))}
								</select>
							</div>
						)}
						
						{searchResult && (
							<p className="text-sm mt-2 text-green-400">
								{searchResult}
							</p>
						)}
					</div>
					<div className="flex justify-end gap-2">
						<button
							type="button"
							className="px-4 py-2 bg-gray-600 hover:bg-gray-500 cursor-pointer"
							onClick={onClose}>
							Cancel
						</button>
						<button
							type="submit"
							className="px-4 py-2 bg-blue-600 hover:bg-blue-500 cursor-pointer">
							Save
						</button>
					</div>
				</form>
			</div>
		</div>
	);
}
