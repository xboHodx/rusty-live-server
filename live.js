(function(){
    const DEBUG = false;
    const chase_buffer_size = 1;

    // ===========================================================================
    // STRING CONSTANTS - All API strings centralized here for easy reference
    // ===========================================================================
    const API = {
        // Request parameters
        PARAMS: {
            SESSION_ID: 'session_id',
            ACTION: 'action',
            ANSWER: 'answer',
            STATUS: 'status',
            END: 'end',
        },
        // Action values
        ACTION: {
            CONNECT: 'connect',
        },
        // Response fields
        RESPONSE: {
            STREAM_NAME: 'stream_name',
            VIDEO_URI: 'video_uri',
            QUESTION: 'question',
            IS_PUBLISHER: 'is_publisher',
            STREAM_STATUS: 'stream_status',
        },
        // Status values
        STATUS: {
            UNREGISTERED: 'unregistered',
            BANNED: 'banned',
            PENDING: 'pending',
            LIVE: 'live',
            PAUSED: 'paused',
            ENDED: 'ended',
        },
        // Legacy support (for backward compatibility during migration)
        LEGACY: {
            // Old parameter names
            RID: 'rid',
            HELLO: 'hello',
            KOTAE: 'Kotae!!',
            WHATTHE: 'WhatThe',
            OWARI: 'Owari',
            // Old response names
            GONAMAE: 'Gonamae',
            SHITSUMON: 'Shitsumon!',
            OKAERI: 'Okaeri',
            JITSUWA: 'jitsuwa',
            // Old status values
            ALL_GOOD: 'all-good',
            KIMI_NO_NA_WA: 'Kimi no Na wa',
            NANKA_IE_YO: 'Nanka ie yo',
            NANINANI: 'naninani',
            CHOTTOMATTE: 'Chottomatte',
            MO_OWARI: 'Mo-owari',
        },
    };

    // ===========================================================================
    // REQUEST BUILDER
    // ===========================================================================
    function request_builder(path){
        ret = {path: path, params:{}};
        ret.add = function(key, value){
            this.params[key] = value;
            return this;
        }
        ret.build = function(){
            return this.path + '?' + Object.keys(this.params).map(function(key){
                return key+"="+encodeURIComponent(ret.params[key])
            }).join("&");
        }
        return ret;
    }

    // ===========================================================================
    // XHR INTERFACE
    // ===========================================================================
    function xhr_request(url, method, body, onsucess, onfail){
        var xhr = new XMLHttpRequest();
        xhr.timeout = 3000;
        xhr.open(method, url);
        xhr.onload = function(){
            if(xhr.status == 200){
                onsucess({
                    status: xhr.status,
                    status_text: xhr.statusText,
                    response: xhr.response
                });
            }
            else{
                onfail({
                    url: url,
                    method: method,
                    body: body,
                    status: xhr.status,
                    status_text: xhr.statusText,
                    response: xhr.response
                });
            }
        };
        xhr.onerror = function(){
            onfail({
                url: url,
                method: method,
                body: body,
                status: xhr.status,
                status_text: xhr.statusText,
                response: xhr.response
            });
        };
        xhr.ontimeout = function(){
            onfail({
                url: url,
                method: method,
                body: body,
                status: xhr.status,
                status_text: xhr.statusText,
                response: xhr.response
            });
        }
        xhr.send(body);
    }

    // ===========================================================================
    // XHR MANAGER - limits concurrency
    // ===========================================================================
    function XHRManager(){
        var xhr_manager={};

        xhr_manager._tick = function(key, interval){
            setTimeout(function(){
                if(DEBUG)
                    console.log("queue size on \"" +key+"\":" +xhr_manager[key].queued.length);
                if(xhr_manager[key].current == null || ["succeeded", "failed"].includes(xhr_manager[key].current.status)){
                    if(xhr_manager[key].queued.length == 0){
                        return;
                    }
                    if(DEBUG){
                        if(xhr_manager[key].current!=null){
                            if(xhr_manager[key].current.status == "succeeded"){
                                xhr_manager[key].completed.push(xhr_manager[key].current)
                            }
                            else{
                                xhr_manager[key].failed.push(xhr_manager[key].current)
                            }
                        }
                    }
                    xhr_manager[key].current = xhr_manager[key].queued.shift();
                    xhr_manager[key].current.run();
                }
                else if(xhr_manager[key].current.status == "pending"){
                    xhr_manager[key].current.run();
                }
                else{
                    return;
                }
            }, interval);
        }

        xhr_manager._push = function(key, task){
            xhr_manager[key].queued.push(task);
        }

        xhr_manager._length = function(key){
            return xhr_manager[key].queued.length;
        }

        xhr_manager.register = function(key, interval = 0){
            if(key in xhr_manager)
                return xhr_manager[key].manager;

            var ret = {
                interval: interval,
            };
            function schedule_imp(url, method, callback, body, retry, interval, failcallback){
                var task = {
                    status: "pending",
                    retry: retry,
                    destination: url,
                    body: body,
                    run: function(){
                        if(this.status == "executing"){
                            throw new Error("Task must not be executed twice concurrently");
                        }
                        this.status = "executing";
                        xhr_request(url, method, body,
                            function(result){
                                task.status = "succeeded";
                                task.result = result
                                callback(result.response);
                                xhr_manager._tick(key, interval);
                            },
                            function(result){
                                if(failcallback){
                                    failcallback(result.status);
                                }
                                console.error(result);
                                console.error("Request to \"" + url + "\" has failed. Retry = "+ task.retry);
                                if(task.retry>0){
                                    task.retry -= 1;
                                    task.status = "pending";
                                }
                                else{
                                    task.status = "failed";
                                    task.result = result;
                                }
                                xhr_manager._tick(key, interval);
                            });
                    },
                    result: null,
                }
                xhr_manager._push(key, task);
                xhr_manager._tick(key, interval);
            }

            ret.schedule_get = function(url, callback, retries = 0, failcallback = null){
                schedule_imp(url, 'GET', callback, null, retries, this.interval, failcallback);
            };

            ret.schedule_post = function(url, callback, body, retries = 0, failcallback = null){
                schedule_imp(url, 'POST', callback, body, retries, this.interval, failcallback);
            };

            ret.set_request_interval = function(interval){
                this.interval = interval;
            }

            xhr_manager[key] = {
                manager: ret,
                current: null,
                queued: [],
                completed: [],
                failed: [],
            }

            return ret;
        }

        return xhr_manager;
    }
    var xhr_manager = XHRManager();
    if(DEBUG)
        window.xhr_manager = xhr_manager;

    // ===========================================================================
    // USER CONFIG - localStorage
    // ===========================================================================
    function init_user_config(){
        function generate_session_id(){
            return Math.floor(Math.random()*Math.pow(2,32)).toString(16).padStart(8, '0')
                  +Math.floor(Math.random()*Math.pow(2,32)).toString(16).padStart(8, '0');
        }

        const session_key = "live_session_id";
        const chat_cfg_key = "chat_config";

        var user = {session_id: generate_session_id(), date: Date.now()};
        var stored_user = JSON.parse(localStorage.getItem(session_key));
        if(stored_user!=null && (Date.now()-stored_user.date)<24*60*60*1000){
            user = stored_user;
        }
        else{
            localStorage.setItem(session_key, JSON.stringify(user));
        }

        var chat_config = {
            name: null,
            live_name: null,
        };
        chat_config = JSON.parse(localStorage.getItem(chat_cfg_key));
        if(chat_config == null){
            chat_config = {
                name: null,
                live_name: null,
            };
        }
        var ret = {
            user: user,
            chat_config: chat_config,
            save: function(){
                localStorage.setItem(chat_cfg_key, JSON.stringify(this.chat_config));
                localStorage.setItem(session_key, JSON.stringify(this.user));
            },
        };
        ret.save();
        return ret;
    }

    var config = init_user_config();
    if(DEBUG)
        window.config = config;

    // ===========================================================================
    // API CLIENT
    // ===========================================================================
    function LiveAPI(_session_id){
        const path = "/api.php";
        const manager = xhr_manager.register('live_api');
        const session_id = _session_id;

        var base_video_url = "livestream.flv";
        var cached_video_uri = "";

        function update_live_name(name){
            document.title = name;
        }

        var ret = {};
        ret.cached_answer = "";

        // Connect to server, get question or video URI
        ret.connect = function(question_callback, video_callback, publisher_callback){
            var url = request_builder(path)
                .add(API.PARAMS.SESSION_ID, session_id)
                .add(API.PARAMS.ACTION, API.ACTION.CONNECT)
                .build();
            manager.schedule_get(url, function(response){
                try{
                    var response_obj = JSON.parse(response);

                    // Try new field names first
                    var streamName = response_obj[API.RESPONSE.STREAM_NAME]
                                || response_obj[API.LEGACY.GONAMAE];
                    var question = response_obj[API.RESPONSE.QUESTION]
                                || response_obj[API.LEGACY.SHITSUMON];
                    var videoUri = response_obj[API.RESPONSE.VIDEO_URI];
                    var isPublisher = response_obj[API.RESPONSE.IS_PUBLISHER];

                    if(streamName){
                        update_live_name(streamName);
                    }
                    if(question){
                        question_callback(question);
                    }
                    else if(videoUri){
                        cached_video_uri = videoUri;
                        video_callback(base_video_url+'?'+cached_video_uri, 'live');
                        if(isPublisher){
                            publisher_callback(true);
                        }
                    }
                }
                catch(error){
                    console.error(error);
                }
            }, 5);
        };

        // Submit answer to quiz question
        ret.submit_answer = function(ans, video_callback, publisher_callback){
            ret.cached_answer = ans;
            var url = request_builder(path)
                .add(API.PARAMS.SESSION_ID, session_id)
                .add(API.PARAMS.ANSWER, ans)
                .build();
            manager.schedule_get(url, function(response){
                try{
                    var response_obj = JSON.parse(response);

                    var streamName = response_obj[API.RESPONSE.STREAM_NAME]
                                || response_obj[API.LEGACY.GONAMAE];
                    var videoUri = response_obj[API.RESPONSE.VIDEO_URI];
                    var isPublisher = response_obj[API.RESPONSE.IS_PUBLISHER];

                    if(streamName){
                        update_live_name(streamName);
                    }
                    cached_video_uri = videoUri;
                    video_callback(base_video_url+'?'+cached_video_uri, 'live');
                    if(isPublisher){
                        publisher_callback(true);
                    }
                }
                catch(error){
                    console.error(error);
                }
            });
        }

        // End the stream (publisher only)
        ret.end_stream = function(end_callback){
            var url = request_builder(path)
                .add(API.PARAMS.SESSION_ID, session_id)
                .add(API.PARAMS.END, 'true')
                .build();
            manager.schedule_get(url, function(response){
                try{
                    var response_obj = JSON.parse(response);
                    end_callback(response_obj);
                }
                catch(error){
                    console.error(error);
                }
            });
        }

        // Check stream status
        ret.check_status = function(video_callback, publisher_callback, end_callback){
            var url = request_builder(path)
                .add(API.PARAMS.SESSION_ID, session_id)
                .add(API.PARAMS.STATUS, 'check')
                .build();
            manager.schedule_get(url, function(response){
                try{
                    var response_obj = JSON.parse(response);

                    // Get stream status field (support both old and new)
                    var statusField = response_obj[API.RESPONSE.STREAM_STATUS]
                                  || response_obj[API.LEGACY.JITSUWA];

                    // Map legacy status values to new ones
                    var status = statusField;
                    switch(statusField){
                        case API.LEGACY.ALL_GOOD:
                        case API.LEGACY.KIMI_NO_NA_WA:
                            status = API.STATUS.BANNED;
                            break;
                        case API.LEGACY.NANKA_IE_YO:
                            status = API.STATUS.PENDING;
                            break;
                        case API.LEGACY.NANINANI:
                            status = API.STATUS.LIVE;
                            break;
                        case API.LEGACY.CHOTTOMATTE:
                            status = API.STATUS.PAUSED;
                            break;
                        case API.LEGACY.MO_OWARI:
                            status = API.STATUS.ENDED;
                            break;
                    }

                    switch(status){
                        case API.STATUS.BANNED:
                            video_callback(base_video_url+'?');
                            break;
                        case API.STATUS.PENDING:
                            ret.submit_answer(ret.cached_answer, video_callback, publisher_callback);
                            break;
                        case API.STATUS.LIVE:
                            video_callback(base_video_url+'?'+cached_video_uri, 'live');
                            break;
                        case API.STATUS.PAUSED:
                            video_callback(base_video_url+'?'+cached_video_uri, 'break');
                            break;
                        case API.STATUS.ENDED:
                            end_callback('ended');
                            break;
                        default:
                            throw new Error("Unknown stream status: " + status);
                    }
                }
                catch(error){
                    console.error(error);
                }
            }, 5);
        }

        // Legacy method names (for backward compatibility)
        ret.hello = ret.connect;
        ret.answer = ret.submit_answer;
        ret.endstream = ret.end_stream;
        ret.checkstatus = ret.check_status;

        return ret;
    }
    var api = LiveAPI(config.user.session_id);
    if(DEBUG)
        window.api = api;

    // ===========================================================================
    // CHAT CLIENT
    // ===========================================================================
    function LiveChat(_session_id){
        const path = "/chat.php";
        const manager = xhr_manager.register('live_chat');
        const session_id = _session_id;

        const url = request_builder(path).add('rid', session_id).build();

        var ret = {};
        ret.stamp_begin = -1;
        ret.stamp_end = -1;

        function act(request, callback, failcallback = undefined){
            manager.schedule_post(url, function(response){
                try{
                    var response_obj = JSON.parse(response);
                    callback(response_obj);
                }
                catch(error){
                    console.error(error);
                }
            }, JSON.stringify(request), 0, failcallback)
        }

        ret.connect = function(name_callback, chat_callback, endchat_callback){
            act({
                action: "hello",
            }, function(response){
                if(response.status == "Nope" || response.status == "error"){
                    endchat_callback();
                    return;
                }
                var messages = response.messages || response.chatmsgs || [];
                if(messages.length>0){
                    ret.stamp_begin = messages[0].stamp;
                    ret.stamp_end = messages[messages.length-1].stamp;
                }
                name_callback(response.name);
                chat_callback(messages);
            }, function(){
                endchat_callback();
            }, 5);
        };

        ret.set_name = function(name, name_callback, reject_callback){
            act({
                action: "setname",
                name: name,
            }, function(response){
                var status = response.status;
                if(status == "Okay" || status == "ok"){
                    name_callback(response.name);
                }
                else{
                    name_callback(response.name);
                    reject_callback(response);
                }
            });
        };

        ret.set_live_name = function(livename, livename_callback, reject_callback){
            act({
                action: "setlivename",
                name: livename,
            }, function(response){
                var status = response.status;
                if(status == "Okay" || status == "ok"){
                    livename_callback(response.name);
                }
                else{
                    livename_callback(response.name);
                    reject_callback(response);
                }
            });
        };

        ret.get_messages = function(direction, newer_callback, older_callback){
            var req = {
                action: "getchat",
            };
            if(direction == "forward"){
                req.next = ret.stamp_end;
            }
            else{
                req.prev = ret.stamp_begin;
            }
            act(req, function(response){
                var messages = response.messages || response.chatmsgs || [];
                if(direction == "forward"){
                    if(messages.length>0)
                        ret.stamp_end = messages[messages.length-1].stamp;
                    newer_callback(messages);
                }
                else{
                    if(messages.length>0)
                        ret.stamp_begin = messages[0].stamp;
                    older_callback(messages);
                }
            });
        };

        ret.send_message = function(chat_message, info_callback, failcallback = undefined){
            act({
                action: "sendchat",
                chat: chat_message,
            }, function(response){
                if(info_callback){
                    info_callback(response.status);
                }
            }, failcallback);
        };

        ret.get_audiences = function(audiences_callback){
            act({
                action: "getaudiences",
            }, function(response){
                audiences_callback(response.audiences);
            });
        };

        ret.save_snapshot = function(info_callback){
            act({
                action: "savesnapshot",
            }, function(response){
                info_callback(response.status);
            })
        };

        // Legacy method names
        ret.hello = ret.connect;
        ret.setname = ret.set_name;
        ret.setlivename = ret.set_live_name;
        ret.getchat = ret.get_messages;
        ret.sendchat = ret.send_message;
        ret.getaudiences = ret.get_audiences;
        ret.savesnapshot = ret.save_snapshot;

        return ret;
    }
    var chat = LiveChat(config.user.session_id);
    if(DEBUG)
        window.chat = chat;

    // ===========================================================================
    // INTERVAL MANAGER
    // ===========================================================================
    function intervalManager(){
        var ret={
            intervals:{},
        };
        ret.register = function(tag, fun, interval){
            ret.intervals[tag] = setInterval(fun, interval*1000);
        };
        ret.deregister = function(tag){
            if(ret.intervals[tag]==undefined){
                return;
            }
            clearInterval(ret.intervals[tag]);
            delete ret[tag];
        };
        return ret;
    }
    var interval_manager = intervalManager();
    if(DEBUG)
        window.interval_manager = interval_manager;

    // ===========================================================================
    // DOM ELEMENTS
    // ===========================================================================
    var dom = {}

    function make_prompt(){
        var prompt_base = document.getElementById('prompt_base');
        var prompt_hint = document.getElementById('prompt_hint');
        var prompt_input = document.getElementById('prompt_input');
        var prompt_confirm = document.getElementById('prompt_confirm');

        return function(hint, callback, placeholder=null){
            prompt_hint.innerText = hint;
            if(placeholder){
                prompt_input.value = placeholder;
                prompt_input.select();
            }
            else{
                prompt_input.value = "";
            }

            prompt_confirm.onclick = function(){
                var input = prompt_input.value;
                if(input=="")
                    input=null;
                prompt_base.classList.add('hidden');
                callback(input);
            }
            prompt_input.onkeydown = function(e){
                if("key" in e){
                    if(e.key == "Enter"){
                        prompt_confirm.onclick();
                    }
                }
                else{
                    if(e.keyCode == 13){
                        prompt_confirm.onclick();
                    }
                }
            }
            prompt_base.classList.remove('hidden');
        };
    }
    var prompt = make_prompt();

    // ===========================================================================
    // CALLBACKS
    // ===========================================================================
    function question_callback(question){
        prompt(question, function(ans){
            api.submit_answer(ans, video_callback, publisher_callback);
        });
    }

    function video_callback(url, type=null){
        if(player!=null && player._config.liveBufferLatencyChasing){
            player_toggle_chasing();
            if(DEBUG)
                console.log("Low latency mode disabled!");
        }
        if(type==null){
            player_load(window.location.href+url+"&session_id="+config.user.session_id);
        }
        else if(type=='break'){
            player_load(window.location.href+url+"&session_id="+config.user.session_id+"&type="+type, false);
            setTimeout(function(){
                api.check_status(video_callback, publisher_callback, end_callback);
            }, 3000);
        }
        else{
            player_load(window.location.href+url+"&session_id="+config.user.session_id+"&type="+type);
        }
    }

    function publisher_callback(is_publisher){
        if(!is_publisher){
            console.error("User is not a publisher");
            return;
        }
        init_publisher_controls();
    }

    function end_callback(reason){
        player_destroy();
        dom.end_overlay.classList.remove('hidden');
        for (const [key, value] of Object.entries(dom)) {
            if(value.tagName == "BUTTON"){
                value.disabled = true;
            }
        }
        interval_manager.deregister('chat_query');
        interval_manager.deregister('watchers_query');
    }

    function name_callback(name){
        if(name==null){
            dom.name_div.onclick = function(){
                prompt("Enter your nickname (cannot be changed during this session):", function(name){
                    if(name == null)
                        return;
                    chat.set_name(name,name_callback,reject_name_callback);
                    dom.name_div.onclick = null;
                    config.chat_config.name = name;
                    config.save();
                }, config.chat_config.name || "Anonymous");
            }
        }
        else{
            dom.name_div.onclick = null;
            dom.name_div.innerText = "Posting as \"" + name + "\"";
        }
    }

    function reject_name_callback(reason){
        alert("Cannot change nickname!\nYour nickname cannot be changed during this session and must be unique.");
        dom.name_div.onclick = function(){
            prompt("Enter your nickname (cannot be changed during this session):", function(name){
                if(name == null)
                    return;
                chat.set_name(name,name_callback,reject_name_callback);
                dom.name_div.onclick = null;
                config.chat_config.name = name;
                config.save();
            }, config.chat_config.name || "Anonymous");
        }
    }

    function livename_callback(name){
        window.title = name;
        dom.btn_set_live_name.onclick = dom.btn_set_live_name.onclick_;
    }

    function reject_livename_callback(reason){
        alert("Permission denied!\nOnly the publisher can change the live stream name.");
    }

    function newer_callback(chat_msgs){
        chat_msg_manager.append(chat_msgs);
    }

    function older_callback(chat_msgs){
        chat_msg_manager.ahead(chat_msgs);
    }

    function chat_sent_callback(status){
        dom.input_box.value = "Sent successfully!"
        setTimeout(function(){
            dom.btn_send_chat.disabled = false;
            dom.input_box.disabled = false;
            dom.input_box.value = "";
            dom.input_box.focus();
        }, 1000);
    }

    function chat_sent_failed_callback(status){
        dom.input_box.value = "Send failed!"
        setTimeout(function(){
            dom.btn_send_chat.disabled = false;
            dom.input_box.disabled = false;
            dom.input_box.value = dom.input_box.value_;
            dom.input_box.focus();
        }, 1000);
    }

    function endchat_callback(){
        dom.side_bar.parentNode.removeChild(dom.side_bar);
        dom.main_container.style.width = "85%";
        dom.main_container.style.marginLeft = "auto";
    }

    function audiences_callback(audiences){
        dom.watchers.innerText = `${audiences.current} viewers`;
    }

    function snapshot_saved_callback(status){
        if(status == "Okay" || status == "ok"){
            alert("Chat history saved successfully");
            dom.btn_save_snapshot.onclick = dom.btn_save_snapshot.onclick_;
        }
        else
            alert("Permission denied!\nOnly the publisher can save chat history.");
    }

    // ===========================================================================
    // CHAT MESSAGE MANAGER
    // ===========================================================================
    function ChatMessageManager(){
        var ret = {};
        ret.bind = function(chatbox){
            ret.chatbox = chatbox;
            var btn = document.createElement('button');
            btn.className = "ahead";
            btn.innerText = "Load earlier messages";
            btn.onclick = function(){
                chat.get_messages("backward", newer_callback, older_callback);
            }
            ret.chatbox.appendChild(btn);
        }
        function to_element(msg){
            var chat_entry = document.createElement('div');
            chat_entry.className = "chatEntry";
            var chat_name = document.createElement('div');
            chat_name.className = "chatName";
            if("name" in msg){
                chat_name.innerText = msg.name + ":";
            }
            else{
                chat_name.innerText = "Anonymous@" + msg.ip + ":";
                chat_name.classList.add('anonymous')
            }
            if(msg.pub){
                chat_name.innerText = "Publisher" + ":";
                chat_name.classList.add('publisher');
            }
            chat_entry.appendChild(chat_name);
            var chat_content = document.createElement('div');
            chat_content.className = 'chatContent';
            chat_content.innerText = msg.content;
            chat_entry.appendChild(chat_content);
            return chat_entry;
        }
        ret.append = function(msgs){
            for(let i=0;i<msgs.length;++i){
                ret.chatbox.appendChild(to_element(msgs[i]));
            }
            dom.chat_box.scrollTop = dom.chat_box.scrollHeight;
            while(dom.chat_box.childElementCount > 3000){
                dom.chat_box.removeChild(dom.chat_box.firstChild);
            }
        }
        ret.ahead = function(msgs){
            var load_ahead_btn = null;
            if(ret.chatbox.firstChild.tagName == "BUTTON"){
                load_ahead_btn = ret.chatbox.removeChild(ret.chatbox.firstChild);
            }
            for(let i=0;i<msgs.length;++i){
                ret.chatbox.insertBefore(to_element(msgs[msgs.length-i-1]), ret.chatbox.firstChild);
            }
            if(load_ahead_btn && msgs.length>=10){
                ret.chatbox.insertBefore(load_ahead_btn, ret.chatbox.firstChild);
            }
            dom.chat_box.scrollTop = 0;
            while(dom.chat_box.childElementCount > 3000){
                dom.chat_box.removeChild(dom.chat_box.firstChild);
            }
        }
        return ret;
    }
    var chat_msg_manager = ChatMessageManager();
    if(DEBUG)
        window.chat_msg_manager = chat_msg_manager;

    // ===========================================================================
    // PLAYER CONTROLS
    // ===========================================================================
    function collect_and_send(){
        dom.btn_send_chat.disabled = true;
        dom.input_box.disabled = true;
        var text = dom.input_box.value;
        chat.send_message(text, chat_sent_callback, chat_sent_failed_callback);
        dom.input_box.value_ = dom.input_box.value;
        dom.input_box.value = "Sending...";
    }

    var player = null;
    var cached_url = null;
    function player_load(url, force_reload = true) {
        if(!force_reload){
            if(cached_url == url && player != null)
                return;
        }
        if(url)
            cached_url = url;
        else
            url = cached_url;
        if(mpegts.isSupported()){
            var mediaDataSource = {
                type: 'flv',
                isLive: true,
                hasAudio: true,
                hasVideo: true,
            };
            mediaDataSource['url'] = url;
            player_load_mds(mediaDataSource);
        }
        else{
            dom.failure_face.classList.remove('hidden');
        }
    }

    function player_load_mds(mediaDataSource) {
        var video = document.getElementsByName('videoElement')[0];
        dom.failure_face.classList.add('hidden');
        video.loop = true;
        var low_latency=false;
        if (typeof player !== "undefined") {
            if (player != null) {
                low_latency = player._config.liveBufferLatencyChasing;
                player.unload();
                player.detachMediaElement();
                player.destroy();
                player = null;
            }
        }
        player = mpegts.createPlayer(mediaDataSource, {
            enableWorker: true,
            liveBufferLatencyChasing: low_latency,
            lazyLoadMaxDuration: 2 * 60,
            seekType: 'range',
            autoCleanupSourceBuffer: true,
            autoCleanupMaxBackwardDuration: 20*60,
            autoCleanupMinBackwardDuration: 15*60,
        });
        if(DEBUG)
            window.player = player;

        player.attachMediaElement(video);
        player.on(mpegts.Events.MEDIA_INFO, function(){
            dom.player_overlay.onclick = function(){
                dom.player_overlay.classList.add('hidden');
                video.controls=true;
                video.autoplay=true;
                player_start();
            }
        });
        player.on(mpegts.Events.LOADING_COMPLETE, function(){
            video.loop = true;
        });
        player.on(mpegts.Events.ERROR, function(){
            dom.player_overlay.classList.add('hidden');
            dom.failure_face.classList.remove('hidden');
            setTimeout(function(){
                api.check_status(video_callback, publisher_callback, end_callback);
            }, 3000);
        });
        if(video.autoplay == true){
            dom.player_overlay.classList.add('hidden');
        }
        player.load();
    }

    function player_start() {
        if(player==null)
            return;
        player.play();
    }

    function player_pause() {
        if(player==null)
            return;
        player.pause();
    }

    function player_toggle_chasing(){
        if(player==null)
            return;
        if(player._config.liveBufferLatencyChasing){
            player._config.liveBufferLatencyChasing = false;
            dom.btn_toggle.textContent = 'Enable low latency';
        }
        else{
            player._config.liveBufferLatencyChasing = true;
            dom.btn_toggle.textContent = 'Disable low latency';
        }
    }

    function player_chase_once(){
        if(player==null)
            return;
        player.currentTime = player.buffered.end(0) - chase_buffer_size;
    }

    function player_destroy() {
        if(player!=null){
            player.pause();
            player.unload();
            player.detachMediaElement();
            player.destroy();
            player = null;
        }
        dom.player_overlay.classList.remove('hidden');
    }

    // ===========================================================================
    // PUBLISHER CONTROLS
    // ===========================================================================
    function init_publisher_controls(){
        if(player != null){
            player.muted = true;
        }
        var controls = document.getElementById('control_group');
        if(controls.childElementCount > 5)
            return;

        // Set live name
        dom.btn_set_live_name = document.createElement('button');
        dom.btn_set_live_name.innerText="Set title";
        dom.btn_set_live_name.title="Set live stream title";
        dom.btn_set_live_name.onclick = function(){
            var place_holder = config.chat_config.live_name || "Normal";
            prompt("Enter live stream title:", function(live_name){
                chat.set_live_name(live_name + "'s live stream", livename_callback, reject_livename_callback);
                config.chat_config.live_name = live_name;
                config.save();
                dom.btn_set_live_name.onclick_ = dom.btn_set_live_name.onclick;
                dom.btn_set_live_name.onclick = null;
            }, place_holder);
        }
        controls.insertBefore(dom.btn_set_live_name, controls.lastChild)

        // Save snapshot
        dom.btn_save_snapshot = document.createElement('button');
        dom.btn_save_snapshot.innerText = "Save chat";
        dom.btn_save_snapshot.title = "Save chat history";
        dom.btn_save_snapshot.onclick = function(){
            chat.save_snapshot(snapshot_saved_callback);
            dom.btn_save_snapshot.onclick_ = dom.btn_save_snapshot.onclick;
            dom.btn_save_snapshot.onclick = null;
        }
        controls.insertBefore(dom.btn_save_snapshot, controls.lastChild)

        // End stream
        dom.btn_end_stream = document.createElement('button');
        dom.btn_end_stream.innerText = "End stream";
        dom.btn_end_stream.title = "End the live stream";
        dom.btn_end_stream.onclick = function(){
            api.end_stream(end_callback);
        }
        controls.insertBefore(dom.btn_end_stream, controls.lastChild)

        dom.name_div.innerText = "Publisher";
        dom.name_div.onclick = null;
    }

    // ===========================================================================
    // DOM INITIALIZATION
    // ===========================================================================
    function init_dom_elements(){
        dom.player_overlay = document.getElementById('overlay');
        dom.failure_face = document.getElementById('failure');
        dom.end_overlay = document.getElementById('oyasumi');
        dom.btn_reload = document.getElementById('btn_reload');
        dom.btn_destroy = document.getElementById('btn_destroy');
        dom.btn_toggle = document.getElementById('btn_toggle_chase');
        dom.btn_chase = document.getElementById('btn_chase');
        dom.main_container = document.getElementById('main');
        dom.side_bar = document.getElementById('side_bar');

        dom.btn_send_chat = document.getElementById('btn_send');
        dom.chat_box = document.getElementById('chat_box');
        dom.input_box = document.getElementById('input_box');
        dom.input_box.value = "";
        dom.name_div = document.getElementById('user_name');
        dom.name_div.innerText = "Anonymous - Click to set nickname";
        dom.chat_overlay = document.getElementById('chat_overlay');
        dom.watchers = document.getElementById('watchers');

        if(DEBUG)
            window._dom = dom;
    }

    function init_dom_events(){
        dom.btn_reload.onclick = function(){
            player_load()
        };
        dom.btn_destroy.onclick = player_destroy;
        dom.btn_toggle.onclick = player_toggle_chasing;
        dom.btn_chase.onclick = player_chase_once;

        dom.chat_overlay.onclick = function(){
            chat.connect(name_callback, chat_callback, endchat_callback);
            dom.chat_overlay.onclick = null;
            dom.chat_overlay.children[0].innerText = "Please wait...";
        }
        chat_msg_manager.bind(dom.chat_box);

        dom.btn_send_chat.onclick = function(){
            collect_and_send();
        }

        dom.input_box.onkeydown = function(e){
            if("key" in e){
                if(e.ctrlKey && e.key == "Enter"){
                    collect_and_send();
                }
            }
            else{
                if(e.ctrlKey && e.keyCode==13){
                    collect_and_send();
                }
            }
        }
    }

    // ===========================================================================
    // INITIALIZATION
    // ===========================================================================
    document.addEventListener('DOMContentLoaded', function () {
        init_dom_elements()
        init_dom_events();
        api.connect(question_callback, video_callback, publisher_callback);
    });

})()
