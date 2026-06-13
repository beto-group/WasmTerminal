async function View({ folderPath }) {
    const domUtils = await dc.require(folderPath + '/src/utils/domUtils.jsx');
    const { ProcessManager_Standalone } = await dc.require(folderPath + '/src/App.jsx');

    return (
        <ProcessManager_Standalone 
            domUtils={domUtils} 
            folderPath={folderPath}
        />
    );
}

return { View };
